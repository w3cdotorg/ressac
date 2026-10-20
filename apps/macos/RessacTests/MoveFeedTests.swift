import Foundation
import Testing

@testable import Ressac

@MainActor @Suite struct MoveFeedTests {
    @Test func move_request_arguments() {
        #expect(RunRequest.moveFeed(id: 7, folder: 3).arguments == ["feed", "move", "7", "3"])
        #expect(RunRequest.moveFeed(id: 7, folder: nil).arguments == ["feed", "move", "7", "root"])
        // No network: a short run. A folder deleted meanwhile exits 2, a sentence and not a wrong command line.
        #expect(RunRequest.moveFeed(id: 7, folder: 3).kind == .short)
        let exitTwo = RunFailure(exitCode: 2, lastStderrLine: "No folder #3.")
        #expect(RunRequest.moveFeed(id: 7, folder: 3).isRefusal(exitTwo))
    }

    @Test func drop_on_own_folder_is_ignored() {
        #expect(FeedDrop.decide(currentFolder: 3, dropFolder: 3) == .ignore)
        // A feed already at the top level dropped on the "Feeds" header stays where it is.
        #expect(FeedDrop.decide(currentFolder: nil, dropFolder: nil) == .ignore)
    }

    @Test func drop_maps_a_feed_onto_a_folder() {
        #expect(FeedDrop.decide(currentFolder: nil, dropFolder: 3) == .move(to: 3))
        #expect(FeedDrop.decide(currentFolder: 2, dropFolder: 3) == .move(to: 3))
        // The "Feeds" header is the root.
        #expect(FeedDrop.decide(currentFolder: 2, dropFolder: nil) == .move(to: nil))
    }

    @Test func payload_round_trips_and_foreign_strings_are_ignored() {
        #expect(FeedDrop.payload(feedId: 42) == "ressac-feed:42")
        #expect(FeedDrop.feedId(in: ["ressac-feed:42"]) == 42)
        #expect(FeedDrop.feedId(in: ["https://example.com", "ressac-feed:9"]) == 9)
        for foreign in ["42", "ressac-feed:", "ressac-feed:x", "ressac-feed:1 2", "ressac-feed:-1", "ressac-feed:+3", "xressac-feed:4", ""] {
            #expect(FeedDrop.feedId(in: [foreign]) == nil, "\(foreign)")
        }
    }

    @Test func selection_follows_the_moved_feed() async throws {
        let (state, _, stub) = try makeState()
        state.selection = .feed(7)
        let outcome = await state.moveFeed(7, to: 3).value
        #expect(outcome == .succeeded)
        #expect(state.selection == .feed(7))
        #expect(try calls(stub).last?.hasSuffix("feed move 7 3") == true)

        // Another feed's move leaves the selection alone.
        _ = await state.moveFeed(8, to: nil).value
        #expect(state.selection == .feed(7))
        #expect(try calls(stub).last?.hasSuffix("feed move 8 root") == true)
    }
}
