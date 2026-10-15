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

    /// Extraction failures are shown inline in the reader, never as a banner (spec §8).
    var showsFailureBanner: Bool { arguments.first != "extract" }

    /// True when `failure` is the store refusing a garden decision or an undo (exit 2, e.g. an
    /// address another active feed already uses), which the app cannot foresee. From any other
    /// call, exit 2 means the app built a wrong command line (spec §8).
    func isRefusal(_ failure: RunFailure) -> Bool {
        failure.exitCode == 2
            && (arguments.starts(with: ["garden", "decide"]) || arguments == ["garden", "undo"])
    }
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
        let process = Process()
        process.executableURL = executable
        process.arguments = ["--db", database.path, "--json"] + request.arguments
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
        let (events, continuation) = AsyncThrowingStream<EngineEvent, any Error>.makeStream()
        let log = Self.log
        let commandLine = request.arguments.joined(separator: " ")
        do {
            try process.run()
        } catch {
            log.error("cannot launch ressac \(commandLine, privacy: .public): \(error.localizedDescription, privacy: .public)")
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
                    log.warning("skipped non-JSON line from ressac: \(line, privacy: .public)")
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

    /// Drains stderr (so the child never blocks on a full pipe), logs it, and keeps the last non-empty line.
    private static func lastLine(of handle: FileHandle, log: Logger) async -> String? {
        var last: String?
        for await line in lines(of: handle) {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            guard !trimmed.isEmpty else { continue }
            log.info("ressac: \(trimmed, privacy: .public)")
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
