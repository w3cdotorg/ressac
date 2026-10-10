import Foundation
import GRDB
import Testing

@testable import Ressac

struct SmokeTests {
    @Test func grdb_is_linked() throws {
        let db = try DatabaseQueue()
        let one = try db.read { try Int.fetchOne($0, sql: "SELECT 1") }
        #expect(one == 1)
    }
}
