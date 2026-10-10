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
