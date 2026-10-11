# Phase 8: `RessacCLI` subprocess runner, NDJSON events, run coordination

Read the plan index first. Global Constraints apply (Subprocess contract). Spec §4 (Data flow: one long run at a time, short runs serialised; Bundling), §6 (the `--json` events), §7 (progress bar), §8 (exit codes, banner, Show log) and §9 (`RessacCLI` against recorded event lines and a stub executable).

**Files (5):**
- Create: `apps/macos/Ressac/Engine/Events.swift`
- Create: `apps/macos/Ressac/Engine/RessacCLI.swift`
- Create: `apps/macos/Ressac/App/RunCoordinator.swift`
- Create: `apps/macos/RessacTests/EngineTests.swift`
- Create: `apps/macos/RessacTests/RunCoordinatorTests.swift`

**Test command:** as in phase 6, narrowed with `-only-testing:RessacTests/<Suite>`:

```sh
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)/apps/macos" && xcodegen generate --quiet && \
  xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test \
    -only-testing:RessacTests/EventTests \
  2>&1 | grep -E '^/.+: error:|✘|Test run|\*\* TEST'
```

**The JSON being mirrored.** `crates/ressac-core/src/service.rs` derives `Serialize` on `Event` with `#[serde(tag = "event", rename_all = "snake_case")]` and no `skip_serializing_if`, so `None` fields are printed as `null` and fields keep their declaration order. `Summary` has no rename: its keys are `total`, `counts` (a `BTreeMap`, so sorted status names such as `"Alive"`), `https_repairs`, `redirect_repairs`, `network_unreachable`. Phase 4 adds `Extracted { entry_id, ok, error }`, `Decided { feed_id, decision }` and `Undone { feed_id }`. Two JSON lines have no known tag: `check --json` prints `{"event":"report",…}` and `import --json` prints the bare report `{"dry_run":…,"folders_added":…,"feeds_added":…,"merged":…,"skipped_existing":…}`; both decode to `.unknown(line)` so phase 11 can read the dry-run counts from it.

**Interface additions to the master** (additive unless noted):
- `enum GardenDecision` lives in `Engine/RessacCLI.swift` rather than `App/AppState.swift`, because `RunRequest.decide(feedId:_:)` needs it first. Same name, same cases.
- `EngineEvent.decode(line:) -> EngineEvent?`: nil for anything that is not a JSON object; `.unknown(String)` holds the **raw line** of a JSON object this build cannot read.
- `RunRequest.showsFailureBanner` (false for `extract`, whose failures are shown inline, spec §8), `RunFailure.message`, and exit code `127` for a binary that cannot be launched.
- `enum RunOutcome { case succeeded, refused, failed(RunFailure) }`; `RunCoordinator.start(_:onEvent:) -> Task<RunOutcome, Never>` (queues synchronously), `perform(_:onEvent:) async -> RunOutcome`, and `onDatabaseChange`. `submit(_:) async -> Bool` is kept and returns true only when the run was accepted **and** exited 0, so `import` then `fetch` can be chained.
- `RunProgress.fraction: Double?` (nil means an indeterminate bar).

### Task 8.1: Decode the NDJSON events

**Interfaces:**
- Consumes: nothing from earlier phases.
- Produces (`Engine/Events.swift`): `struct Summary: Decodable, Sendable, Equatable { total: Int; counts: [String: Int]; httpsRepairs: Int; redirectRepairs: Int; networkUnreachable: Bool }`; `enum EngineEvent: Decodable, Sendable, Equatable` with `started(total:)`, `feedDone(feedId:title:error:)`, `networkUnreachable(failed:attempted:)`, `probing(what:total:)`, `waybackDone(found:)`, `finished(summary: Summary?)`, `extracted(entryId:ok:error:)`, `decided(feedId:decision:)`, `undone(feedId:)`, `unknown(String)`, and `static func decode(line: String) -> EngineEvent?`.

- [ ] **Step 1: Write the failing tests** `apps/macos/RessacTests/EngineTests.swift`. The lines are exactly what `serde_json` prints:

```swift
import Foundation
import Testing

@testable import Ressac

@Suite struct EventTests {
    let summaryJSON = #"{"total":11,"counts":{"Alive":9,"Dead":2},"https_repairs":1,"redirect_repairs":0,"network_unreachable":false}"#

    /// The exact lines `serde_json` prints for the core's `Event` (field order as declared, `null` for `None`).
    @Test func every_core_event_decodes() {
        let summary = Summary(
            total: 11, counts: ["Alive": 9, "Dead": 2], httpsRepairs: 1, redirectRepairs: 0, networkUnreachable: false)
        let cases: [(String, EngineEvent)] = [
            (#"{"event":"started","total":3}"#, .started(total: 3)),
            (#"{"event":"feed_done","feed_id":1,"title":"xkcd.com","error":null}"#, .feedDone(feedId: 1, title: "xkcd.com", error: nil)),
            (#"{"event":"feed_done","feed_id":2,"title":"B","error":"HTTP 404"}"#, .feedDone(feedId: 2, title: "B", error: "HTTP 404")),
            (#"{"event":"network_unreachable","failed":4,"attempted":5}"#, .networkUnreachable(failed: 4, attempted: 5)),
            (#"{"event":"probing","what":"full articles","total":2}"#, .probing(what: "full articles", total: 2)),
            (#"{"event":"wayback_done","found":1}"#, .waybackDone(found: 1)),
            (#"{"event":"finished","summary":\#(summaryJSON)}"#, .finished(summary: summary)),
            (#"{"event":"finished"}"#, .finished(summary: nil)),
            (#"{"event":"extracted","entry_id":42,"ok":false,"error":"HTTP 404."}"#, .extracted(entryId: 42, ok: false, error: "HTTP 404.")),
            (#"{"event":"extracted","entry_id":42,"ok":true,"error":null}"#, .extracted(entryId: 42, ok: true, error: nil)),
            (#"{"event":"decided","feed_id":7,"decision":"bury"}"#, .decided(feedId: 7, decision: "bury")),
            (#"{"event":"undone","feed_id":null}"#, .undone(feedId: nil)),
            (#"{"event":"undone","feed_id":7}"#, .undone(feedId: 7)),
        ]
        for (line, event) in cases {
            #expect(EngineEvent.decode(line: line) == event, "\(line)")
        }
    }

    @Test func unknown_objects_are_kept_and_garbage_is_dropped() {
        let report = #"{"event":"report","summary":{}}"#
        let importReport = #"{"dry_run":true,"folders_added":7,"feeds_added":11,"merged":0,"skipped_existing":0}"#
        #expect(EngineEvent.decode(line: report) == .unknown(report))
        #expect(EngineEvent.decode(line: importReport) == .unknown(importReport))
        #expect(EngineEvent.decode(line: #"{"event":"started"}"#) == .unknown(#"{"event":"started"}"#))
        for garbage in ["", "not json", "[1,2]", "42", #"{"event":"fini"#, "\u{1b}[2K progress"] {
            #expect(EngineEvent.decode(line: garbage) == nil, "\(garbage)")
        }
    }
}
```

- [ ] **Step 2: Run** `-only-testing:RessacTests/EventTests`. Expected: `error: cannot find 'EngineEvent' in scope`.

- [ ] **Step 3: Implement** `apps/macos/Ressac/Engine/Events.swift`:

```swift
import Foundation

/// The core's `service::Summary`.
struct Summary: Decodable, Sendable, Equatable {
    var total: Int
    /// Status name ("Dead", "Alive", …) → number of active feeds.
    var counts: [String: Int]
    var httpsRepairs: Int
    var redirectRepairs: Int
    var networkUnreachable: Bool

    private enum CodingKeys: String, CodingKey {
        case total, counts
        case httpsRepairs = "https_repairs"
        case redirectRepairs = "redirect_repairs"
        case networkUnreachable = "network_unreachable"
    }
}

/// One NDJSON line printed by `ressac --json` (the core's `service::Event`,
/// `#[serde(tag = "event", rename_all = "snake_case")]`).
enum EngineEvent: Decodable, Sendable, Equatable {
    case started(total: Int)
    case feedDone(feedId: Int64, title: String, error: String?)
    case networkUnreachable(failed: Int, attempted: Int)
    case probing(what: String, total: Int)
    case waybackDone(found: Int)
    case finished(summary: Summary?)
    case extracted(entryId: Int64, ok: Bool, error: String?)
    case decided(feedId: Int64, decision: String)
    case undone(feedId: Int64?)
    /// A JSON object this build does not know (for example the `import --json` report,
    /// which has no `event` key). Holds the raw line.
    case unknown(String)

    private enum CodingKeys: String, CodingKey {
        case event, total, title, error, failed, attempted, what, found, summary, ok, decision
        case feedId = "feed_id"
        case entryId = "entry_id"
    }

    private struct UnknownTag: Error {}

    init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        switch try c.decode(String.self, forKey: .event) {
        case "started":
            self = .started(total: try c.decode(Int.self, forKey: .total))
        case "feed_done":
            self = .feedDone(
                feedId: try c.decode(Int64.self, forKey: .feedId),
                title: try c.decode(String.self, forKey: .title),
                error: try c.decodeIfPresent(String.self, forKey: .error))
        case "network_unreachable":
            self = .networkUnreachable(
                failed: try c.decode(Int.self, forKey: .failed), attempted: try c.decode(Int.self, forKey: .attempted))
        case "probing":
            self = .probing(what: try c.decode(String.self, forKey: .what), total: try c.decode(Int.self, forKey: .total))
        case "wayback_done":
            self = .waybackDone(found: try c.decode(Int.self, forKey: .found))
        case "finished":
            self = .finished(summary: try c.decodeIfPresent(Summary.self, forKey: .summary))
        case "extracted":
            self = .extracted(
                entryId: try c.decode(Int64.self, forKey: .entryId),
                ok: try c.decode(Bool.self, forKey: .ok),
                error: try c.decodeIfPresent(String.self, forKey: .error))
        case "decided":
            self = .decided(
                feedId: try c.decode(Int64.self, forKey: .feedId), decision: try c.decode(String.self, forKey: .decision))
        case "undone":
            self = .undone(feedId: try c.decodeIfPresent(Int64.self, forKey: .feedId))
        default:
            throw UnknownTag()
        }
    }

    /// Decodes one stdout line. Nil for anything that is not a JSON object (logged and
    /// skipped by the caller, Review Focus #5); `.unknown(line)` for an object this build
    /// cannot read.
    static func decode(line: String) -> EngineEvent? {
        let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
        let data = Data(trimmed.utf8)
        guard trimmed.hasPrefix("{"),
            (try? JSONSerialization.jsonObject(with: data)) is [String: Any]
        else { return nil }
        return (try? JSONDecoder().decode(EngineEvent.self, from: data)) ?? .unknown(trimmed)
    }
}
```

- [ ] **Step 4: Run.** Expected: `✔ Test run with 2 tests in 1 suite passed`.

- [ ] **Step 5: Commit.**

```sh
git add apps/macos/Ressac/Engine/Events.swift apps/macos/RessacTests/EngineTests.swift
git commit -m "macOS app: decode the ressac NDJSON events"
```

### Task 8.2: Run the binary and stream its events

**Interfaces:**
- Consumes: `EngineEvent.decode(line:)`; Foundation `Process`, `Pipe`, `FileHandle.readabilityHandler`; `Synchronization.Mutex`; `os.Logger`.
- Produces (`Engine/RessacCLI.swift`):
  - `enum GardenDecision: String, Sendable, CaseIterable { case keep, repair, bury }`, `enum RunKind { case long, short }`;
  - `struct RunRequest: Sendable, Equatable { var arguments: [String]; var kind: RunKind }` with `fetch(feeds:)`, `fetch(folder:)`, `importOPML(_:dryRun:)`, `export(to:cemetery:)`, `extract(entryId:)`, `decide(feedId:_:)`, `undo()`, `applySafe()`, `migrate()` and `showsFailureBanner`;
  - `struct RunFailure: Error, Equatable, Sendable { let exitCode: Int32; let lastStderrLine: String? }` with `message`;
  - `actor RessacCLI { init(executable: URL, database: URL); func run(_ request: RunRequest) -> AsyncThrowingStream<EngineEvent, any Error>; static func bundledExecutable() -> URL?; static func lines(of: FileHandle) -> AsyncStream<String> }`;
  - `final class LineSplitter: Sendable { func append(_ data: Data) -> [String]; func finish() -> String? }`.
  - Test helpers `struct Stub` (a `/bin/sh` script written to a temp dir; `$4` is the subcommand because `$1 $2 $3` are `--db <path> --json`) and `collect(_:)`.

**Argument mapping** (the runner prepends `--db <path> --json`): `fetch(feeds: [])` → `fetch --force` (a manual refresh ignores the 30-minute rule, spec §2); `fetch(feeds: [3, 8])` → `fetch --feed 3 --feed 8`; `fetch(folder: 4)` → `fetch --folder 4`; `importOPML` → `import PATH [--dry-run]` (long, except the dry run); `export` → `export --output PATH [--cemetery]`; `extract` → `extract ID`; `decide` → `garden decide ID keep|repair|bury`; `undo` → `garden undo`; `applySafe` → `garden apply-safe`; `migrate` → `migrate`.

- [ ] **Step 1: Write the failing tests.** Append to `EngineTests.swift`. `runner_survives_garbage_and_signals` pins Review Focus #5: a non-JSON line, an unknown event, a partial last line and a `SIGKILL`.

```swift
/// A stand-in for the `ressac` binary: a `/bin/sh` script in a fresh temp directory.
/// `$1 $2 $3` are `--db <path> --json`, so the subcommand starts at `$4`.
struct Stub {
    let dir: URL
    let executable: URL
    var database: URL { dir.appending(path: "ressac.sqlite") }

    init(_ body: String) throws {
        dir = FileManager.default.temporaryDirectory.appending(path: "ressac-stub-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        executable = dir.appending(path: "ressac")
        try ("#!/bin/sh\n" + body + "\n").write(to: executable, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: executable.path)
    }

    var cli: RessacCLI { RessacCLI(executable: executable, database: database) }

    func file(_ name: String) -> URL { dir.appending(path: name) }
}

/// Collects a run's events, then its error (nil after a 0 exit).
func collect(_ stream: AsyncThrowingStream<EngineEvent, any Error>) async -> ([EngineEvent], (any Error)?) {
    var events: [EngineEvent] = []
    do {
        for try await event in stream { events.append(event) }
        return (events, nil)
    } catch {
        return (events, error)
    }
}

@Suite struct LineSplitterTests {
    @Test func splits_on_newlines_and_keeps_partial_lines() {
        let splitter = LineSplitter()
        let euro = Array("€".utf8)  // 3 bytes, cut between two reads
        #expect(splitter.append(Data("one\r\ntwo\n\nthr".utf8)) == ["one", "two", ""])
        #expect(splitter.append(Data("ee ".utf8 + euro[0..<1])) == [])
        #expect(splitter.append(Data(euro[1...] + Array("\npartial".utf8))) == ["three €"])
        #expect(splitter.finish() == "partial")
        #expect(splitter.finish() == nil)
    }
}

@Suite struct RunRequestTests {
    @Test func factories_build_the_cli_arguments() {
        #expect(RunRequest.fetch(feeds: []) == RunRequest(arguments: ["fetch", "--force"], kind: .long))
        #expect(RunRequest.fetch(feeds: [3, 8]).arguments == ["fetch", "--feed", "3", "--feed", "8"])
        #expect(RunRequest.fetch(folder: 4) == RunRequest(arguments: ["fetch", "--folder", "4"], kind: .long))
        let opml = URL(filePath: "/tmp/My Feeds.opml")
        #expect(RunRequest.importOPML(opml, dryRun: true) == RunRequest(arguments: ["import", "/tmp/My Feeds.opml", "--dry-run"], kind: .short))
        #expect(RunRequest.importOPML(opml, dryRun: false) == RunRequest(arguments: ["import", "/tmp/My Feeds.opml"], kind: .long))
        #expect(RunRequest.export(to: opml, cemetery: true).arguments == ["export", "--output", "/tmp/My Feeds.opml", "--cemetery"])
        #expect(RunRequest.extract(entryId: 42) == RunRequest(arguments: ["extract", "42"], kind: .short))
        #expect(RunRequest.decide(feedId: 7, .repair).arguments == ["garden", "decide", "7", "repair"])
        #expect(RunRequest.undo().arguments == ["garden", "undo"])
        #expect(RunRequest.applySafe().arguments == ["garden", "apply-safe"])
        #expect(RunRequest.migrate() == RunRequest(arguments: ["migrate"], kind: .short))
        #expect(!RunRequest.extract(entryId: 1).showsFailureBanner && RunRequest.undo().showsFailureBanner)
    }
}

@Suite struct RessacCLITests {
    @Test func bundled_executable_is_found() {
        #expect(RessacCLI.bundledExecutable()?.lastPathComponent == "ressac")
    }

    @Test func runner_passes_db_and_json_before_the_subcommand() async throws {
        let stub = try Stub(#"printf '%s\n' "$@" > "$(dirname "$0")/args""#)
        let (events, error) = await collect(await stub.cli.run(.fetch(feeds: [3])))
        #expect(events.isEmpty && error == nil)
        let args = try String(contentsOf: stub.file("args"), encoding: .utf8).split(separator: "\n")
        #expect(args == ["--db", "\(stub.database.path)", "--json", "fetch", "--feed", "3"])
    }

    @Test func events_arrive_while_the_process_runs() async throws {
        let stub = try Stub("""
            echo '{"event":"started","total":1}'
            sleep 2
            echo '{"event":"finished"}'
            """)
        let start = ContinuousClock.now
        var firstAt: Duration?
        for try await event in await stub.cli.run(.fetch(feeds: [])) where firstAt == nil {
            #expect(event == .started(total: 1))
            firstAt = ContinuousClock.now - start
        }
        #expect(try #require(firstAt) < .seconds(1))
    }

    /// Review Focus #5: garbage, a partial last line and a signal death must not crash the app.
    @Test func runner_survives_garbage_and_signals() async throws {
        let stub = try Stub("""
            echo '{"event":"started","total":2}'
            echo 'warning: this is not JSON'
            echo '{"event":"mystery","x":1}'
            echo '{"event":"feed_done","feed_id":1,"title":"A","error":null}'
            echo 'about to crash' >&2
            printf '{"event":"feed_done","feed_id":2,"ti'
            kill -9 $$
            """)
        let (events, error) = await collect(await stub.cli.run(.fetch(feeds: [])))
        #expect(events == [
            .started(total: 2),
            .unknown(#"{"event":"mystery","x":1}"#),
            .feedDone(feedId: 1, title: "A", error: nil),
        ])
        #expect(error as? RunFailure == RunFailure(exitCode: -9, lastStderrLine: "about to crash"))
    }

    @Test func last_line_without_newline_is_still_read() async throws {
        let stub = try Stub(#"printf '{"event":"undone","feed_id":null}'"#)
        let (events, error) = await collect(await stub.cli.run(.undo()))
        #expect(events == [.undone(feedId: nil)] && error == nil)
    }

    @Test func nonzero_exit_reports_the_last_stderr_line() async throws {
        let stub = try Stub("""
            echo 'first problem' >&2
            echo 'error: cannot open database /x' >&2
            echo '' >&2
            exit 1
            """)
        let (_, error) = await collect(await stub.cli.run(.migrate()))
        #expect(error as? RunFailure == RunFailure(exitCode: 1, lastStderrLine: "error: cannot open database /x"))
    }

    @Test func usage_error_and_interruption_keep_their_codes() async throws {
        let stub = try Stub(#"[ "$4" = extract ] && { echo 'This entry has no link.' >&2; exit 2; }; exit 130"#)
        let (_, usage) = await collect(await stub.cli.run(.extract(entryId: 1)))
        #expect(usage as? RunFailure == RunFailure(exitCode: 2, lastStderrLine: "This entry has no link."))
        let (_, interrupted) = await collect(await stub.cli.run(.undo()))
        #expect(interrupted as? RunFailure == RunFailure(exitCode: 130, lastStderrLine: nil))
    }

    @Test func missing_executable_is_a_launch_failure() async throws {
        let cli = RessacCLI(executable: URL(filePath: "/nonexistent/ressac"), database: URL(filePath: "/tmp/x.sqlite"))
        let (events, error) = await collect(await cli.run(.migrate()))
        let failure = try #require(error as? RunFailure)
        #expect(events.isEmpty && failure.exitCode == 127)
        #expect(failure.message.hasPrefix("Cannot launch ressac"))
    }

    @Test func cancelling_the_consumer_interrupts_the_process() async throws {
        let stub = try Stub("""
            trap 'echo interrupted > "$(dirname "$0")/trapped"; exit 130' INT
            echo '{"event":"started","total":1}'
            while true; do sleep 0.1; done
            """)
        let cli = stub.cli
        let (started, startedSignal) = AsyncStream<Void>.makeStream()
        let consumer = Task {
            for try await _ in await cli.run(.fetch(feeds: [])) { startedSignal.yield() }
        }
        for await _ in started { break }  // the trap is set and the stream is being read
        consumer.cancel()
        let deadline = ContinuousClock.now + .seconds(3)
        while !FileManager.default.fileExists(atPath: stub.file("trapped").path), ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(50))
        }
        #expect(FileManager.default.fileExists(atPath: stub.file("trapped").path))
    }
}
```

- [ ] **Step 2: Run** `-only-testing:RessacTests/LineSplitterTests -only-testing:RessacTests/RunRequestTests -only-testing:RessacTests/RessacCLITests`. Expected: `error: cannot find type 'RessacCLI' in scope` (and `LineSplitter`, `RunRequest`).

- [ ] **Step 3: Implement** `apps/macos/Ressac/Engine/RessacCLI.swift`:

```swift
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
```

- [ ] **Step 4: Run.** Expected: `✔ Test run with 11 tests in 3 suites passed`. The log shows `[engine] skipped non-JSON line from ressac: warning: this is not JSON`: that is the skip being logged.

- [ ] **Step 5: Commit.**

```sh
git add apps/macos/Ressac/Engine/RessacCLI.swift apps/macos/RessacTests/EngineTests.swift
git commit -m "macOS app: run the bundled ressac and stream its events, surviving garbage and signals"
```

### Task 8.3: Coordinate runs: one long run, FIFO short runs, progress, banners

**Interfaces:**
- Consumes: `RessacCLI.run(_:)`, `RunRequest`, `RunFailure`, `EngineEvent`.
- Produces (`App/RunCoordinator.swift`):
  - `struct RunProgress: Equatable, Sendable { var done: Int; var total: Int; var label: String; var fraction: Double? }`;
  - `struct Banner: Equatable, Sendable { let message: String; let showLog: Bool }`;
  - `enum RunOutcome: Equatable, Sendable { case succeeded, refused, failed(RunFailure) }`;
  - `@MainActor @Observable final class RunCoordinator` with `init(cli: RessacCLI)`, `private(set) var isLongRunActive: Bool`, `private(set) var progress: RunProgress?`, `var banner: Banner?`, `var onDatabaseChange: (@MainActor () -> Void)?`, `func submit(_:) async -> Bool`, `func perform(_:onEvent:) async -> RunOutcome`, `@discardableResult func start(_:onEvent:) -> Task<RunOutcome, Never>`.
  - Test helper `@MainActor func eventually(timeout:_:) async -> Bool`, used again by phase 9.

**Behaviour:**
- A long run while another is active returns `.refused` at once; the menus disable their triggers through `isLongRunActive`.
- Short runs wait for the previous short run, in the order `start` was called, and may overlap a long run.
- Progress (long runs only): `started` → "Fetching feeds" 0/total; `feed_done` and `extracted` add one; `probing("full articles")` → "Fetching full articles" (spec §6), any other probe → "Checking <what>"; cleared when the run ends.
- A failure sets `Banner(message: lastStderrLine ?? fallback, showLog: true)`, except for `extract`. Exit 2 is also logged as a fault (a bug in the app's call, spec §8). `network_unreachable` shows its own banner (`showLog: false`), which survives the end of its successful run. Any other success clears the banner.
- `onDatabaseChange` fires after every run and, during a run, at most once a second after `feed_done`, `extracted`, `decided`, `undone` or `finished`.

- [ ] **Step 1: Write the failing tests** `apps/macos/RessacTests/RunCoordinatorTests.swift`:

```swift
import Foundation
import Testing

@testable import Ressac

/// Polls a main-actor condition (observations and runs finish asynchronously).
@MainActor
func eventually(timeout: Duration = .seconds(3), _ condition: () -> Bool) async -> Bool {
    let deadline = ContinuousClock.now + timeout
    while !condition() {
        if ContinuousClock.now > deadline { return false }
        try? await Task.sleep(for: .milliseconds(20))
    }
    return true
}

@MainActor @Suite struct RunCoordinatorTests {
    /// `fetch` sleeps so that it stays active; `extract ID` logs its start and end; `undo` fails.
    let script = """
        log="$(dirname "$0")/log"
        case "$4" in
          fetch)
            echo '{"event":"started","total":2}'
            echo '{"event":"feed_done","feed_id":1,"title":"A","error":null}'
            sleep 1
            echo '{"event":"feed_done","feed_id":2,"title":"B","error":null}'
            echo '{"event":"finished"}' ;;
          extract)
            echo "start $5" >> "$log"; sleep 0.2; echo "end $5" >> "$log"
            echo '{"event":"extracted","entry_id":'"$5"',"ok":false,"error":"HTTP 404."}'
            exit 1 ;;
          garden)
            [ "$5" = undo ] && { echo 'error: nothing to undo here' >&2; exit 1; }
            echo '{"event":"undone","feed_id":null}' ;;
        esac
        """

    @Test func long_run_is_refused_while_another_is_active() async throws {
        let stub = try Stub(script)
        let runs = RunCoordinator(cli: stub.cli)
        let first = runs.start(.fetch(feeds: []))
        #expect(runs.isLongRunActive)
        #expect(await runs.perform(.fetch(folder: 1)) == .refused)
        #expect(await first.value == .succeeded)
        #expect(!runs.isLongRunActive)
        #expect(await runs.submit(.fetch(feeds: [1])))
    }

    @Test func progress_follows_the_events() async throws {
        let stub = try Stub(script)
        let runs = RunCoordinator(cli: stub.cli)
        let run = runs.start(.fetch(feeds: []))
        #expect(await eventually { runs.progress == RunProgress(done: 1, total: 2, label: "Fetching feeds") })
        #expect(runs.progress?.fraction == 0.5)
        _ = await run.value
        #expect(runs.progress == nil)
    }

    @Test func short_runs_run_one_at_a_time_in_order() async throws {
        let stub = try Stub(script)
        let runs = RunCoordinator(cli: stub.cli)
        let tasks = [3, 1, 2].map { runs.start(.extract(entryId: Int64($0))) }
        for task in tasks { _ = await task.value }
        let log = try String(contentsOf: stub.file("log"), encoding: .utf8)
        #expect(log == "start 3\nend 3\nstart 1\nend 1\nstart 2\nend 2\n")
    }

    @Test func short_runs_overlap_a_long_run() async throws {
        let stub = try Stub(script)
        let runs = RunCoordinator(cli: stub.cli)
        let fetch = runs.start(.fetch(feeds: []))
        var events: [EngineEvent] = []
        let outcome = await runs.perform(.decide(feedId: 1, .keep)) { events.append($0) }
        #expect(outcome == .succeeded && events == [.undone(feedId: nil)])
        #expect(runs.isLongRunActive)  // the fetch is still sleeping
        _ = await fetch.value
    }

    @Test func failure_shows_a_banner_and_the_next_success_clears_it() async throws {
        let stub = try Stub(script)
        let runs = RunCoordinator(cli: stub.cli)
        #expect(await runs.perform(.undo()) == .failed(RunFailure(exitCode: 1, lastStderrLine: "error: nothing to undo here")))
        #expect(runs.banner == Banner(message: "error: nothing to undo here", showLog: true))
        #expect(await runs.submit(.applySafe()))
        #expect(runs.banner == nil)
    }

    @Test func extraction_failure_stays_out_of_the_banner() async throws {
        let stub = try Stub(script)
        let runs = RunCoordinator(cli: stub.cli)
        var events: [EngineEvent] = []
        let outcome = await runs.perform(.extract(entryId: 9)) { events.append($0) }
        #expect(outcome == .failed(RunFailure(exitCode: 1, lastStderrLine: nil)))
        #expect(events == [.extracted(entryId: 9, ok: false, error: "HTTP 404.")])
        #expect(runs.banner == nil)
    }

    @Test func network_warning_survives_its_own_successful_run() async throws {
        let stub = try Stub("""
            echo '{"event":"started","total":5}'
            echo '{"event":"network_unreachable","failed":4,"attempted":5}'
            echo '{"event":"finished"}'
            """)
        let runs = RunCoordinator(cli: stub.cli)
        #expect(await runs.submit(.fetch(feeds: [])))
        #expect(runs.banner?.message == "The network looks unreachable: 4 of 5 feeds could not connect.")
    }

    @Test func observations_are_told_after_each_run() async throws {
        let stub = try Stub(script)
        let runs = RunCoordinator(cli: stub.cli)
        var notices = 0
        runs.onDatabaseChange = { notices += 1 }
        _ = await runs.submit(.applySafe())
        _ = await runs.submit(.undo())
        #expect(notices == 2)
    }
}
```

- [ ] **Step 2: Run** `-only-testing:RessacTests/RunCoordinatorTests`. Expected: `error: cannot find 'RunCoordinator' in scope`.

- [ ] **Step 3: Implement** `apps/macos/Ressac/App/RunCoordinator.swift`:

```swift
import Foundation
import Observation
import os

struct RunProgress: Equatable, Sendable {
    var done: Int
    var total: Int
    var label: String

    /// Nil while nothing countable has happened (an indeterminate bar).
    var fraction: Double? {
        total > 0 && done > 0 ? min(1, Double(done) / Double(total)) : nil
    }
}

/// The non-modal banner under the toolbar (spec §8).
struct Banner: Equatable, Sendable {
    let message: String
    let showLog: Bool
}

enum RunOutcome: Equatable, Sendable {
    case succeeded
    /// A long run was asked for while another one was active.
    case refused
    case failed(RunFailure)
}

/// Owns every `ressac` subprocess the app starts: at most one long run, short runs in
/// FIFO order, progress, banners, and change notices for the observations.
@MainActor @Observable final class RunCoordinator {
    private(set) var isLongRunActive = false
    private(set) var progress: RunProgress?
    var banner: Banner?

    /// Called during runs (at most once a second) and after each run, so that GRDB
    /// observations refetch what the subprocess wrote.
    @ObservationIgnored var onDatabaseChange: (@MainActor () -> Void)?

    @ObservationIgnored private let cli: RessacCLI
    @ObservationIgnored private var shortTail: Task<RunOutcome, Never>?
    @ObservationIgnored private var lastChangeNotice = ContinuousClock.now
    @ObservationIgnored private var bannerSetDuringRun = false
    private static let log = Logger(subsystem: "org.w3cdotorg.ressac", category: "runs")

    init(cli: RessacCLI) {
        self.cli = cli
    }

    /// Runs `request` and waits for it. True only when it ran and exited 0; false when it
    /// was refused (a long run is active) or failed (the banner says why).
    func submit(_ request: RunRequest) async -> Bool {
        await start(request).value == .succeeded
    }

    func perform(_ request: RunRequest, onEvent: (@MainActor (EngineEvent) -> Void)? = nil) async -> RunOutcome {
        await start(request, onEvent: onEvent).value
    }

    /// Queues `request` synchronously (so callers control the order) and returns its task.
    @discardableResult
    func start(_ request: RunRequest, onEvent: (@MainActor (EngineEvent) -> Void)? = nil) -> Task<RunOutcome, Never> {
        switch request.kind {
        case .long:
            guard !isLongRunActive else {
                Self.log.info("refused \(request.arguments.joined(separator: " "), privacy: .public): a long run is active")
                return Task { .refused }
            }
            isLongRunActive = true
            return Task {
                let outcome = await execute(request, onEvent: onEvent)
                isLongRunActive = false
                progress = nil
                return outcome
            }
        case .short:
            let previous = shortTail
            let task = Task {
                _ = await previous?.value
                return await execute(request, onEvent: onEvent)
            }
            shortTail = task
            return task
        }
    }

    private func execute(_ request: RunRequest, onEvent: (@MainActor (EngineEvent) -> Void)?) async -> RunOutcome {
        if request.kind == .long {
            bannerSetDuringRun = false
        }
        var outcome = RunOutcome.succeeded
        do {
            for try await event in await cli.run(request) {
                if request.kind == .long {
                    apply(event)
                }
                onEvent?(event)
                noteChange(after: event)
            }
        } catch let failure as RunFailure {
            outcome = .failed(failure)
        } catch {
            outcome = .failed(RunFailure(exitCode: 1, lastStderrLine: error.localizedDescription))
        }
        onDatabaseChange?()
        lastChangeNotice = .now
        switch outcome {
        case .succeeded, .refused:
            if !bannerSetDuringRun || request.kind == .short {
                banner = nil
            }
        case let .failed(failure):
            if failure.exitCode == 2 {
                Self.log.fault("usage error from ressac \(request.arguments.joined(separator: " "), privacy: .public)")
            }
            if request.showsFailureBanner {
                banner = Banner(message: failure.message, showLog: true)
            }
        }
        return outcome
    }

    private func apply(_ event: EngineEvent) {
        switch event {
        case let .started(total):
            progress = RunProgress(done: 0, total: total, label: "Fetching feeds")
        case .feedDone, .extracted:
            progress?.done += 1
        case let .probing(what, total):
            progress = RunProgress(
                done: 0, total: total, label: what == "full articles" ? "Fetching full articles" : "Checking \(what)")
        case let .networkUnreachable(failed, attempted):
            banner = Banner(
                message: "The network looks unreachable: \(failed) of \(attempted) feeds could not connect.",
                showLog: false)
            bannerSetDuringRun = true
        case .waybackDone, .finished, .decided, .undone, .unknown:
            break
        }
    }

    private func noteChange(after event: EngineEvent) {
        switch event {
        case .feedDone, .extracted, .decided, .undone, .finished:
            if ContinuousClock.now - lastChangeNotice >= .seconds(1) {
                lastChangeNotice = .now
                onDatabaseChange?()
            }
        default:
            break
        }
    }
}
```

- [ ] **Step 4: Run** the whole suite. Expected: `✔ Test run with 42 tests in 9 suites passed` in about 10 s (the stubs sleep).

- [ ] **Step 5: Commit.**

```sh
git add apps/macos/Ressac/App/RunCoordinator.swift apps/macos/RessacTests/RunCoordinatorTests.swift
git commit -m "macOS app: coordinate ressac runs with progress, banners and change notices"
```

### Verification gate

```sh
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/macos && xcodegen generate && \
  xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test 2>&1 | tee "${TMPDIR:-/tmp}/ressac-xcodebuild.log" | tail -3
grep -E '^/.+/apps/macos/.+: (warning|error):' "${TMPDIR:-/tmp}/ressac-xcodebuild.log" || echo "no warnings in our sources"
```

Expected: Rust checks unchanged, `** TEST SUCCEEDED **` with 42 Swift tests, `no warnings in our sources`. Push.

### Notes for the executor

- **Do not read the pipes with `FileHandle.bytes.lines`.** It buffers: with a stub that prints a line, sleeps 2 s and prints another, the first event arrived only after 2.2 s, at exit. `events_arrive_while_the_process_runs` catches that regression. `readabilityHandler` delivers data as soon as it is written; `availableData` is empty exactly once, at end of file.
- **`LineSplitter` keeps bytes, not text,** until a newline arrives, so a UTF-8 character cut between two reads is not replaced by U+FFFD. The partial last line is yielded at end of file, then decoded (a valid object without a trailing newline is accepted; a truncated one is skipped).
- **Signals.** `kill -9 $$` in the stub ends `/bin/sh` itself: `terminationReason == .uncaughtSignal` and `terminationStatus == 9`, mapped to `exitCode == -9`. A process that cannot start (`/nonexistent/ressac`) throws from `Process.run()`; the stream then finishes with `RunFailure(exitCode: 127, …)`.
- **Cancellation.** Cancelling the consumer's task terminates the stream, which sends `SIGINT` (`Process.interrupt()`): the CLI treats it as Ctrl-C (stop scheduling, exit 130).
- **Concurrency checks.** With complete strict concurrency, `Process` (macOS 27 SDK) can be captured in the `@Sendable` termination and `onTermination` closures without warnings; `LineSplitter` is `Sendable` through `Mutex` from the `Synchronization` module (macOS 15+).
- **`stderr` is drained concurrently** with `stdout` (`async let`), so a chatty helper never blocks on a full pipe; every stderr line is logged with `os.Logger` (subsystem `org.w3cdotorg.ressac`, category `engine`), visible in Console.app.
- **Swift Testing runs suites in parallel.** Each test builds its own `Stub` directory, so nothing is shared. The FIFO test uses `start(_:)`, which enqueues synchronously; never rely on the order in which separate `Task { }`s reach an `await`.
