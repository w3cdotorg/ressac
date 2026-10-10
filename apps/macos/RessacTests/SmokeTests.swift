import Foundation
import GRDB
import Testing

@testable import Ressac

struct SmokeTests {
    @Test func bundled_helper_is_embedded_and_runs() throws {
        let helper = Bundle.main.bundleURL.appending(path: "Contents/Helpers/ressac")
        #expect(FileManager.default.isExecutableFile(atPath: helper.path))
        let process = Process()
        process.executableURL = helper
        process.arguments = ["--version"]
        let out = Pipe()
        process.standardOutput = out
        try process.run()
        process.waitUntilExit()
        let text = String(decoding: out.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
        #expect(process.terminationStatus == 0)
        #expect(text.hasPrefix("ressac "))
    }

    @Test func grdb_is_linked() throws {
        let db = try DatabaseQueue()
        let one = try db.read { try Int.fetchOne($0, sql: "SELECT 1") }
        #expect(one == 1)
    }
}
