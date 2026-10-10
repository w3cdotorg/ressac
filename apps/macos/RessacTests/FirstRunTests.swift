import AppKit
import Foundation
import Testing
@testable import Ressac

@Suite struct BundledFontTests {
    @Test func bundledFontsShipWithTheirLicences() {
        #expect(NSFont(name: "Newsreader", size: 12) != nil, "ATSApplicationFontsPath did not register Newsreader")
        #expect(NSFont(name: "Atkinson Hyperlegible Next", size: 12) != nil, "ATSApplicationFontsPath did not register Atkinson")
        for licence in ["Newsreader-OFL", "AtkinsonHyperlegibleNext-OFL"] {
            #expect(Bundle.main.url(forResource: licence, withExtension: "txt", subdirectory: "Fonts") != nil, "missing \(licence).txt")
        }
        let newsreader = ArticleTemplate.fontFaces(for: .newsreader)
        #expect(newsreader.components(separatedBy: "@font-face").count == 3)
        #expect(newsreader.contains("font-style: italic"))
        #expect(newsreader.contains("data:font/ttf;base64,"))
        #expect(ArticleTemplate.fontFaces(for: .atkinson).contains(#"font-family: "Atkinson Hyperlegible Next""#))
        #expect(ArticleTemplate.fontFaces(for: .charter).isEmpty)
    }
}


@Suite struct DatabaseLocationTests {
    @Test func environmentThenSettingThenTheSharedDefault() throws {
        let suite = "ressac.tests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let shared = URL.homeDirectory.appending(path: "Library/Application Support/Ressac/ressac.sqlite")
        #expect(DatabaseLocation.current(environment: [:], defaults: defaults) == shared)
        defaults.set("  ", forKey: DatabaseLocation.defaultsKey)
        #expect(DatabaseLocation.current(environment: [:], defaults: defaults) == shared)
        defaults.set("~/Feeds/other.sqlite", forKey: DatabaseLocation.defaultsKey)
        #expect(DatabaseLocation.current(environment: [:], defaults: defaults) == URL.homeDirectory.appending(path: "Feeds/other.sqlite"))
        #expect(DatabaseLocation.current(environment: ["RESSAC_DB": "/tmp/copy.sqlite"], defaults: defaults) == URL(filePath: "/tmp/copy.sqlite"))
        #expect(DatabaseLocation.current(environment: ["RESSAC_DB": ""], defaults: defaults) == URL.homeDirectory.appending(path: "Feeds/other.sqlite"))
    }
}
private let sampleLine = #"{"dry_run":true,"folders_added":16,"feeds_added":186,"merged":[["Daring Fireball (1)","Daring Fireball"]],"skipped_existing":3}"#
private let sampleReport = ImportReport(dryRun: true, foldersAdded: 16, feedsAdded: 186,
                                        merged: [["Daring Fireball (1)", "Daring Fireball"]], skippedExisting: 3)

/// A throwaway directory with executable stand-ins for the bundled `ressac`.
private func stubDirectory() throws -> URL {
    let dir = FileManager.default.temporaryDirectory.appending(path: "ressac-stub-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    let scripts = [
        "ok": "#!/bin/sh\necho 'starting' >&2\necho '\(sampleLine)'\n",
        "bad": "#!/bin/sh\necho 'error: cannot read /nowhere.opml' >&2\nexit 1\n",
        "killed": "#!/bin/sh\nkill -9 $$\n",
    ]
    for (name, body) in scripts {
        let url = dir.appending(path: name)
        try body.write(to: url, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: url.path(percentEncoded: false))
    }
    return dir
}

@MainActor private final class Recorder {
    var requests: [RunRequest] = []
    var results: [Bool] = []
    func submit(_ request: RunRequest) async -> Bool {
        requests.append(request)
        return results.isEmpty ? true : results.removeFirst()
    }
}

@Suite struct ImportPreviewTests {
    @Test func decodesTheCLIJSONLine() throws {
        #expect(try ImportPreview.decode(Data("\(sampleLine)\n".utf8)) == sampleReport)
        #expect(throws: ImportPreviewFailure.self) { try ImportPreview.decode(Data("not json\n".utf8)) }
    }

    @Test func argumentsAskForAJSONDryRun() {
        let args = ImportPreview.arguments(database: URL(filePath: "/tmp/a b/ressac.sqlite"), file: URL(filePath: "/tmp/f.opml"))
        #expect(args == ["--db", "/tmp/a b/ressac.sqlite", "--json", "import", "--dry-run", "/tmp/f.opml"])
    }

    @Test func runsTheHelperAndReportsFailures() async throws {
        let dir = try stubDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let db = dir.appending(path: "ressac.sqlite")
        let file = dir.appending(path: "f.opml")
        #expect(try await ImportPreview.run(executable: dir.appending(path: "ok"), database: db, file: file) == sampleReport)
        await #expect(throws: ImportPreviewFailure(exitCode: 1, message: "cannot read /nowhere.opml")) {
            try await ImportPreview.run(executable: dir.appending(path: "bad"), database: db, file: file)
        }
        await #expect(throws: ImportPreviewFailure(exitCode: -9, message: "The import preview failed.")) {
            try await ImportPreview.run(executable: dir.appending(path: "killed"), database: db, file: file)
        }
    }
}

@Suite @MainActor struct OPMLFlowTests {
    private let file = URL(filePath: "/tmp/vienna.opml")

    @Test func summaryDescribesTheDryRun() {
        #expect(OPMLFlow.summary(of: sampleReport) == [
            "186 feeds in 16 new folders.",
            "1 duplicate inside the file will be merged:",
            "“Daring Fireball (1)” into “Daring Fireball”",
            "3 feeds already in Ressac will be skipped.",
        ])
        let nothing = ImportReport(dryRun: true, foldersAdded: 0, feedsAdded: 0, merged: [], skippedExisting: 1)
        #expect(OPMLFlow.summary(of: nothing) == ["No new feeds in this file.", "1 feed already in Ressac will be skipped."])
    }

    @Test func confirmingImportsThenFetchesEverything() async {
        let recorder = Recorder()
        let flow = OPMLFlow(preview: { _ in sampleReport }, submit: recorder.submit)
        #expect(await flow.open([URL(filePath: "/tmp/notes.txt"), file]))
        #expect(flow.step == .confirming(file, sampleReport))
        await flow.confirm()
        #expect(recorder.requests == [.importOPML(file, dryRun: false), .fetch(feeds: [])])
        #expect(flow.step == .idle)
    }

    @Test func aFailedImportDoesNotFetch() async {
        let recorder = Recorder()
        recorder.results = [false]
        let flow = OPMLFlow(preview: { _ in sampleReport }, submit: recorder.submit)
        await flow.open([file])
        await flow.confirm()
        #expect(recorder.requests == [.importOPML(file, dryRun: false)])
        #expect(flow.step == .idle)
    }

    @Test func previewFailuresAreShownAndNothingIsWritten() async {
        let recorder = Recorder()
        let flow = OPMLFlow(preview: { _ in throw ImportPreviewFailure(exitCode: 1, message: "invalid OPML") }, submit: recorder.submit)
        await flow.open([file])
        #expect(flow.failureMessage == "invalid OPML")
        flow.cancel()
        #expect(flow.step == .idle)
        #expect(recorder.requests.isEmpty)
    }

    @Test func onlyOPMLFilesAreAccepted() async {
        let flow = OPMLFlow(preview: { _ in sampleReport }, submit: { _ in true })
        #expect(await flow.open([URL(filePath: "/tmp/feeds.xml"), URL(string: "https://example.org/a.opml")!]) == false)
        #expect(flow.step == .idle)
        #expect(OPMLFlow.firstOPML(in: [URL(filePath: "/tmp/A.OPML")]) == URL(filePath: "/tmp/A.OPML"))
    }

    @Test func exportRunsRessacExport() async {
        let recorder = Recorder()
        let flow = OPMLFlow(preview: { _ in sampleReport }, submit: recorder.submit)
        let target = URL(filePath: "/tmp/Ressac.opml")
        #expect(await flow.export(to: target))
        #expect(recorder.requests == [.export(to: target, cemetery: false)])
    }
}
