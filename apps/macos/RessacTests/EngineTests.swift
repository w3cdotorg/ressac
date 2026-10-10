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
