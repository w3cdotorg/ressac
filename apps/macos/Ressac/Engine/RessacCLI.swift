import Foundation
import Synchronization
import os

/// A garden decision, as `ressac garden decide` spells it.
enum GardenDecision: String, Sendable, CaseIterable {
    case keep, repair, bury
}

/// Long runs (fetch, import) are exclusive; short ones (extract, garden, export, migrate)
/// may overlap a long run and are serialised among themselves (spec §4, Data flow).
enum RunKind: Sendable, Equatable {
    case long, short
}

/// Feed addresses can carry secrets (`user:pass@host`, `?token=…`): anything logged goes through here.
/// Words that look like an address (with a scheme, or a scheme-less `host/path`, `host?query`,
/// `user@host`) keep scheme, host and path; userinfo, query and fragment are dropped
/// (`?…` marks a query that was there). Other words are left as they are.
enum LogRedaction {
    /// A command line for the log. The address `add` takes is redacted whatever it looks like (a
    /// dotless host, brackets, spaces); every other argument goes through `redact`.
    static func commandLine(_ arguments: [String]) -> String {
        arguments.enumerated().map { index, argument in
            index == 1 && arguments.first == "add" ? redactAddress(argument) : redact(argument)
        }.joined(separator: " ")
    }

    static func redact(_ text: String) -> String {
        text.split(separator: " ", omittingEmptySubsequences: false).map { word in
            looksLikeAddress(word) ? redactAddress(String(word)) : String(word)
        }.joined(separator: " ")
    }

    /// A scheme, or no scheme but a host-like first segment (a `.` or `localhost`, maybe `user@` and
    /// `:port`) and something that can carry a secret or a path (`/`, `?`, `#`, `@`).
    private static func looksLikeAddress(_ word: Substring) -> Bool {
        if word.contains("://") { return true }
        guard let first = word.first, first != "/", first != "~",
              word.contains(where: { "/?#@".contains($0) }) else { return false }
        let authority = word[..<(word.firstIndex(of: "/") ?? word.endIndex)]
        let hostAndMore = authority[(authority.lastIndex(of: "@").map { authority.index(after: $0) } ?? authority.startIndex)...]
        let hostEnd = hostAndMore.firstIndex { $0 == "?" || $0 == "#" || $0 == ":" } ?? hostAndMore.endIndex
        let host = hostAndMore[..<hostEnd]
        return host.contains(".") || host == "localhost"
    }

    /// Userinfo first (up to the last `@` of the authority, even past a `?` or `#` in a password),
    /// then the query and the fragment.
    static func redactAddress(_ word: String) -> String {
        let schemeEnd = word.range(of: "://")?.upperBound ?? word.startIndex
        let head = word[..<schemeEnd]
        var rest = word[schemeEnd...]
        let authorityEnd = rest.firstIndex(of: "/") ?? rest.endIndex
        if let at = rest[..<authorityEnd].lastIndex(of: "@") {
            rest = rest[rest.index(after: at)...]
        }
        let end = rest.firstIndex { $0 == "?" || $0 == "#" } ?? rest.endIndex
        let hasQuery = end < rest.endIndex && rest[end] == "?"
        return head + rest[..<end] + (hasQuery ? "?…" : "")
    }
}

/// One `ressac` invocation, without the `--db … --json` prefix the runner adds.
struct RunRequest: Sendable, Equatable {
    var arguments: [String]
    var kind: RunKind

    /// All feeds when `feeds` is empty. A manual refresh ignores the 30-minute rule,
    /// hence `--force` (`--feed` implies it).
    static func fetch(feeds: [Int64]) -> RunRequest {
        let arguments = feeds.isEmpty ? ["fetch", "--force"] : ["fetch"] + feeds.flatMap { ["--feed", String($0)] }
        return RunRequest(arguments: arguments, kind: .long)
    }

    static func fetch(folder: Int64) -> RunRequest {
        RunRequest(arguments: ["fetch", "--folder", String(folder)], kind: .long)
    }

    static func importOPML(_ url: URL, dryRun: Bool) -> RunRequest {
        RunRequest(arguments: ["import", url.path] + (dryRun ? ["--dry-run"] : []), kind: dryRun ? .short : .long)
    }

    static func export(to url: URL, cemetery: Bool) -> RunRequest {
        RunRequest(arguments: ["export", "--output", url.path] + (cemetery ? ["--cemetery"] : []), kind: .short)
    }

    static func extract(entryId: Int64) -> RunRequest {
        RunRequest(arguments: ["extract", String(entryId)], kind: .short)
    }

    static func decide(feedId: Int64, _ decision: GardenDecision) -> RunRequest {
        RunRequest(arguments: ["garden", "decide", String(feedId), decision.rawValue], kind: .short)
    }

    static func undo() -> RunRequest {
        RunRequest(arguments: ["garden", "undo"], kind: .short)
    }

    static func applySafe() -> RunRequest {
        RunRequest(arguments: ["garden", "apply-safe"], kind: .short)
    }

    static func migrate() -> RunRequest {
        RunRequest(arguments: ["migrate"], kind: .short)
    }

    /// Refresh, then compose today's edition (spec §6). Long: it fetches every due feed.
    static func buildEdition(rebuild: Bool = false) -> RunRequest {
        RunRequest(arguments: ["edition", "build"] + (rebuild ? ["--rebuild"] : []), kind: .long)
    }

    /// Forces a feed's rhythm group, or gives it back to the core with `nil` ("auto").
    static func setCadence(feedId: Int64, _ group: CadenceGroup?) -> RunRequest {
        RunRequest(arguments: ["feed", "cadence", String(feedId), group?.rawValue ?? "auto"], kind: .short)
    }

    /// Puts a feed on a sea (a theme key), or takes it off every sea with `nil` ("none").
    static func setTheme(feedId: Int64, key: String?) -> RunRequest {
        RunRequest(arguments: ["feed", "theme", String(feedId), key ?? "none"], kind: .short)
    }

    /// Moves a feed into a folder, or to the top level with `nil` ("root"). No network: a short run.
    static func moveFeed(id: Int64, folder: Int64?) -> RunRequest {
        RunRequest(arguments: ["feed", "move", String(id), folder.map(String.init) ?? "root"], kind: .short)
    }

    /// Adds the feed at `address` (a feed or a site; the CLI discovers and fetches it). Long: it
    /// fetches, so it cannot overlap another fetch. `theme` is a theme key; nil inherits from the site.
    static func addFeed(address: String, theme: String?) -> RunRequest {
        RunRequest(arguments: ["add", address] + (theme.map { ["--theme", $0] } ?? []), kind: .long)
    }

    /// Extraction failures are shown inline in the reader and add failures in the Add Feed sheet,
    /// never as a banner (spec §8; add-feed spec §7).
    var showsFailureBanner: Bool { !["extract", "add"].contains(arguments.first) }

    /// True when `failure` is the store refusing something the app cannot foresee (exit 2): an address that announces
    /// no feed or is already followed (`add`), a garden decision or an undo (an address another
    /// active feed already uses), an edition
    /// that is not due or cannot be built offline, an override on a feed buried meanwhile, a theme or a folder deleted meanwhile.
    /// From any other call, exit 2 means the app built a wrong command line (spec §8).
    func isRefusal(_ failure: RunFailure) -> Bool {
        failure.exitCode == 2 && Self.refusable.contains { arguments.starts(with: $0) }
    }

    private static let refusable: [[String]] = [
        ["garden", "decide"], ["garden", "undo"], ["edition", "build"], ["feed", "cadence"],
        ["feed", "theme"], ["feed", "move"], ["add"],
    ]
}

/// A run that did not exit 0. A signal death has `exitCode = -signal`; a binary that
/// could not be launched has `exitCode = 127`.
struct RunFailure: Error, Equatable, Sendable {
    let exitCode: Int32
    let lastStderrLine: String?

    /// The banner text (spec §8).
    var message: String {
        if let lastStderrLine { return lastStderrLine }
        return exitCode < 0
            ? "ressac stopped unexpectedly (signal \(-exitCode))."
            : "ressac failed with exit code \(exitCode)."
    }
}

/// Runs the bundled `ressac` binary and streams its NDJSON events (spec §4, §8).
actor RessacCLI {
    let executable: URL
    let database: URL

    private static let log = Logger(subsystem: "org.w3cdotorg.ressac", category: "engine")

    init(executable: URL, database: URL) {
        self.executable = executable
        self.database = database
    }

    /// `Ressac.app/Contents/Helpers/ressac`, or nil when the bundle lacks it.
    static func bundledExecutable() -> URL? {
        let url = Bundle.main.bundleURL.appending(path: "Contents/Helpers/ressac")
        return FileManager.default.isExecutableFile(atPath: url.path) ? url : nil
    }

    /// Events in order. The stream finishes after a 0 exit and throws `RunFailure`
    /// otherwise. Lines that are not JSON objects are logged and skipped. Cancelling the
    /// consumer sends SIGINT, which the CLI treats as Ctrl-C.
    func run(_ request: RunRequest) -> AsyncThrowingStream<EngineEvent, any Error> {
        let (process, stdout, stderr, exits) = makeProcess(arguments: request.arguments)
        let (events, continuation) = AsyncThrowingStream<EngineEvent, any Error>.makeStream()
        let log = Self.log
        let commandLine = LogRedaction.commandLine(request.arguments)
        do {
            try process.run()
        } catch {
            log.error("cannot launch ressac \(commandLine, privacy: .public): \(LogRedaction.redact(error.localizedDescription), privacy: .public)")
            continuation.finish(throwing: RunFailure(
                exitCode: 127, lastStderrLine: "Cannot launch ressac: \(error.localizedDescription)"))
            return events
        }
        Task {
            async let lastStderrLine = Self.lastLine(of: stderr.fileHandleForReading, log: log)
            for await line in Self.lines(of: stdout.fileHandleForReading) {
                if let event = EngineEvent.decode(line: line) {
                    continuation.yield(event)
                } else if !line.isEmpty {
                    log.warning("skipped non-JSON line from ressac: \(LogRedaction.redact(line), privacy: .public)")
                }
            }
            let stderrLine = await lastStderrLine
            var exit: (status: Int32, reason: Process.TerminationReason) = (0, .exit)
            for await status in exits { exit = status }
            switch (exit.reason, exit.status) {
            case (.exit, 0):
                continuation.finish()
            case (.uncaughtSignal, let signal):
                log.error("ressac \(commandLine, privacy: .public) died from signal \(signal)")
                continuation.finish(throwing: RunFailure(exitCode: -signal, lastStderrLine: stderrLine))
            case (_, let code):
                log.error("ressac \(commandLine, privacy: .public) exited with \(code)")
                continuation.finish(throwing: RunFailure(exitCode: code, lastStderrLine: stderrLine))
            }
        }
        continuation.onTermination = { termination in
            if case .cancelled = termination, process.isRunning {
                process.interrupt()
            }
        }
        return events
    }

    /// A configured, not yet started `ressac --db … --json <arguments>`, with its pipes and
    /// an exit stream the termination handler fills.
    private func makeProcess(arguments: [String]) -> (
        process: Process, stdout: Pipe, stderr: Pipe,
        exits: AsyncStream<(status: Int32, reason: Process.TerminationReason)>
    ) {
        let process = Process()
        process.executableURL = executable
        process.arguments = ["--db", database.path, "--json"] + arguments
        process.standardInput = FileHandle.nullDevice
        let stdout = Pipe()
        let stderr = Pipe()
        process.standardOutput = stdout
        process.standardError = stderr
        let (exits, exitSignal) = AsyncStream<(status: Int32, reason: Process.TerminationReason)>.makeStream()
        process.terminationHandler = { process in
            exitSignal.yield((process.terminationStatus, process.terminationReason))
            exitSignal.finish()
        }
        return (process, stdout, stderr, exits)
    }

    /// Runs a command that prints one document rather than events (`chart`) and returns its
    /// whole stdout. Throws `RunFailure` on a non-zero exit, or a launch failure (127).
    /// Cancelling the caller sends SIGINT.
    func capture(_ arguments: [String]) async throws -> Data {
        let (process, stdout, stderr, exits) = makeProcess(arguments: arguments)
        let log = Self.log
        let commandLine = LogRedaction.commandLine(arguments)
        do {
            try process.run()
        } catch {
            log.error("cannot launch ressac \(commandLine, privacy: .public): \(LogRedaction.redact(error.localizedDescription), privacy: .public)")
            throw RunFailure(exitCode: 127, lastStderrLine: "Cannot launch ressac: \(error.localizedDescription)")
        }
        let output = stdout.fileHandleForReading
        let (data, stderrLine, exit) = await withTaskCancellationHandler {
            async let lastStderrLine = Self.lastLine(of: stderr.fileHandleForReading, log: log)
            // A blocking read to EOF: it holds one cooperative pool thread for the whole run.
            let data = await Task.detached { (try? output.readToEnd()) ?? Data() }.value
            var exit: (status: Int32, reason: Process.TerminationReason) = (0, .exit)
            for await status in exits { exit = status }
            return (data, await lastStderrLine, exit)
        } onCancel: {
            if process.isRunning { process.interrupt() }
        }
        switch (exit.reason, exit.status) {
        case (.exit, 0):
            return data
        case (.uncaughtSignal, let signal):
            log.error("ressac \(commandLine, privacy: .public) died from signal \(signal)")
            throw RunFailure(exitCode: -signal, lastStderrLine: stderrLine)
        case (_, let code):
            log.error("ressac \(commandLine, privacy: .public) exited with \(code)")
            throw RunFailure(exitCode: code, lastStderrLine: stderrLine)
        }
    }

    /// Drains stderr (so the child never blocks on a full pipe), logs it, and keeps the last non-empty line.
    private static func lastLine(of handle: FileHandle, log: Logger) async -> String? {
        var last: String?
        for await line in lines(of: handle) {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            guard !trimmed.isEmpty else { continue }
            log.info("ressac: \(LogRedaction.redact(trimmed), privacy: .public)")
            last = trimmed
        }
        return last
    }

    /// Lines as soon as they are written. `FileHandle.bytes` would buffer them until the
    /// pipe closes, which freezes the progress bar.
    static func lines(of handle: FileHandle) -> AsyncStream<String> {
        let (stream, continuation) = AsyncStream<String>.makeStream()
        let splitter = LineSplitter()
        handle.readabilityHandler = { handle in
            let data = handle.availableData
            if data.isEmpty {
                handle.readabilityHandler = nil
                if let rest = splitter.finish() { continuation.yield(rest) }
                continuation.finish()
            } else {
                for line in splitter.append(data) { continuation.yield(line) }
            }
        }
        return stream
    }
}

/// Splits a byte stream on `\n`. Bytes after the last newline wait for more data,
/// so a UTF-8 character cut between two reads is never mangled.
final class LineSplitter: Sendable {
    private let pending = Mutex(Data())

    func append(_ data: Data) -> [String] {
        pending.withLock { buffer in
            buffer.append(data)
            var lines: [String] = []
            while let newline = buffer.firstIndex(of: 0x0A) {
                lines.append(Self.text(buffer[buffer.startIndex..<newline]))
                buffer.removeSubrange(buffer.startIndex...newline)
            }
            return lines
        }
    }

    /// The partial last line, if any.
    func finish() -> String? {
        pending.withLock { buffer in
            defer { buffer.removeAll() }
            return buffer.isEmpty ? nil : Self.text(buffer[...])
        }
    }

    private static func text(_ bytes: Data.SubSequence) -> String {
        var line = String(decoding: bytes, as: UTF8.self)
        if line.hasSuffix("\r") { line.removeLast() }
        return line
    }
}
