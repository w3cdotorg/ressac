import Foundation
import Testing

@testable import Ressac

@MainActor @Suite struct AddFeedTests {
    private static let added = """
        echo '{"event":"started","total":1}'
        echo '{"event":"feed_done","feed_id":42,"title":"Daring Fireball","error":null}'
        echo '{"event":"added","id":42,"title":"Daring Fireball","xml_url":"https://df.example/feed","page":131,"theme":"Tech, Apple and geeks","territory":null,"discovered":true,"like":"Six Colors"}'
        """

    @Test func add_request_arguments() {
        #expect(RunRequest.addFeed(address: "df.example", theme: nil).arguments == ["add", "df.example"])
        #expect(RunRequest.addFeed(address: "df.example", theme: "tech").arguments == ["add", "df.example", "--theme", "tech"])
        // The CLI fetches the feed it inserts: a long run. Its refusals (exit 2) are sentences.
        #expect(RunRequest.addFeed(address: "x", theme: nil).kind == .long)
        let refusal = RunFailure(exitCode: 2, lastStderrLine: "Already followed: Daring Fireball.")
        #expect(RunRequest.addFeed(address: "x", theme: nil).isRefusal(refusal))
        // Errors stay in the sheet, never in the window's banner.
        #expect(!RunRequest.addFeed(address: "x", theme: nil).showsFailureBanner)
    }

    @Test func added_event_decodes() {
        let line = #"{"event":"added","id":7,"title":"T","xml_url":"https://a.example/f","page":120,"theme":"Tech","territory":null,"discovered":false,"like":"Sibling"}"#
        #expect(EngineEvent.decode(line: line)
            == .added(AddedFeed(id: 7, title: "T", theme: "Tech", like: "Sibling")))
        let bare = #"{"event":"added","id":8,"title":"U","xml_url":"https://a.example/g","page":121,"theme":null,"territory":null,"discovered":true}"#
        #expect(EngineEvent.decode(line: bare) == .added(AddedFeed(id: 8, title: "U", theme: nil, like: nil)))
    }

    @Test func empty_address_disables_add() {
        let model = AddFeedModel()
        #expect(!model.canAdd)
        model.address = "   \n"
        #expect(!model.canAdd)
        model.address = " example.com "
        #expect(model.canAdd)
        model.isRunning = true
        #expect(!model.canAdd)
    }

    @Test func clipboard_prefill_only_for_web_addresses() {
        #expect(AddFeedModel.clipboardAddress("  https://example.com/feed.xml\n") == "https://example.com/feed.xml")
        #expect(AddFeedModel.clipboardAddress("http://example.com") == "http://example.com")
        #expect(AddFeedModel.clipboardAddress("example.com") == nil)
        #expect(AddFeedModel.clipboardAddress("ftp://example.com/x") == nil)
        #expect(AddFeedModel.clipboardAddress("some words https://example.com") == nil)
        #expect(AddFeedModel.clipboardAddress(nil) == nil)
    }

    @Test func refusal_sentence_shown_in_sheet() async throws {
        let (state, _, _) = try makeState(stub: """
            echo 'Already followed: Daring Fireball.' >&2
            exit 2
            """)
        state.pendingSheet = .addFeed
        let model = AddFeedModel()
        model.address = "https://df.example"
        #expect(await model.add(to: state) == false)
        #expect(model.errorMessage == "Already followed: Daring Fireball.")
        #expect(!model.showsLog)  // a refusal says it all
        #expect(state.pendingSheet == .addFeed)  // the sheet stays open
        #expect(state.runs.banner == nil)  // the sentence stays in the sheet
        #expect(!model.isRunning)
    }

    @Test func failure_keeps_the_sheet_and_offers_the_log() async throws {
        let (state, _, _) = try makeState(stub: """
            echo 'Could not reach https://df.example.' >&2
            exit 1
            """)
        state.pendingSheet = .addFeed
        let model = AddFeedModel()
        model.address = "https://df.example"
        #expect(await model.add(to: state) == false)
        #expect(model.errorMessage == "Could not reach https://df.example.")
        #expect(model.showsLog)
    }

    @Test func success_selects_the_new_feed_and_reports_the_theme() async throws {
        let (state, _, stub) = try makeState(stub: Self.added + "\necho \"$*\" >> \"$(dirname \"$0\")/calls\"")
        state.pendingSheet = .addFeed
        let model = AddFeedModel()
        model.address = " df.example "
        model.themeKey = nil
        #expect(await model.add(to: state))
        #expect(state.selection == .feed(42))
        #expect(state.pendingSheet == nil)
        #expect(state.addFeedNote == "In Tech, Apple and geeks, like Six Colors.")
        #expect(model.errorMessage == nil)
        // The address is trimmed before it reaches the CLI.
        #expect(try calls(stub).last?.hasSuffix("add df.example") == true)
    }

    @Test func note_wording() {
        #expect(AddFeedNote.text(for: AddedFeed(id: 1, title: "A", theme: "Tech", like: "B")) == "In Tech, like B.")
        #expect(AddFeedNote.text(for: AddedFeed(id: 1, title: "A", theme: "Tech", like: nil)) == "In Tech.")
        #expect(AddFeedNote.text(for: AddedFeed(id: 1, title: "A", theme: nil, like: nil)) == "Added to Uncharted waters.")
    }
}

@Suite struct LogRedactionTests {
    @Test func userinfo_query_and_fragment_are_removed() {
        #expect(LogRedaction.redact("add https://user:pass@host.example:8080/feed.xml?token=abc#top --theme tech")
            == "add https://host.example:8080/feed.xml?… --theme tech")
        #expect(LogRedaction.redact("add https://u:p@host.example?token=abc") == "add https://host.example?…")
        #expect(LogRedaction.redact("Could not reach https://host.example/f?k=v.") == "Could not reach https://host.example/f?…")
    }

    @Test func plain_addresses_and_other_arguments_are_unchanged() {
        #expect(LogRedaction.redact("add https://host.example/a/b.xml") == "add https://host.example/a/b.xml")
        #expect(LogRedaction.redact("add https://host.example/@me/feed") == "add https://host.example/@me/feed")
        #expect(LogRedaction.redact("feed theme 7 ai-labs a@b?c") == "feed theme 7 ai-labs a@b?c")
        #expect(LogRedaction.redact("add example.com") == "add example.com")
    }
}
