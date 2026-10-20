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

    @Test func add_request_passes_the_folder_id_only_below_the_top_level() {
        #expect(RunRequest.addFeed(address: "a", theme: nil, folder: 7).arguments == ["add", "a", "--folder", "7"])
        #expect(RunRequest.addFeed(address: "a", theme: "tech", folder: 7).arguments
            == ["add", "a", "--theme", "tech", "--folder", "7"])
        #expect(RunRequest.addFeed(address: "a", theme: "tech", folder: nil).arguments == ["add", "a", "--theme", "tech"])
    }

    @Test func the_folder_pop_up_lists_the_top_level_then_every_folder_in_sidebar_order() {
        func folder(_ id: Int64, _ parent: Int64?, _ name: String, _ position: Int) -> FolderItem {
            FolderItem(folderId: id, parentId: parent, name: name, position: position, page: nil)
        }
        let snapshot = SidebarSnapshot(
            folders: [folder(1, nil, "News", 1), folder(2, 1, "Sub", 0), folder(3, 2, "Deep", 0), folder(4, nil, "Blogs", 0)],
            feeds: [])
        let choices = FolderChoice.choices(from: SidebarNode.tree(from: snapshot))
        #expect(choices.map(\.folderId) == [nil, 4, 1, 2, 3])
        #expect(choices.map(\.depth) == [0, 1, 1, 2, 3])
        #expect(choices.map(\.title) == ["Feeds", "\u{2003}Blogs", "\u{2003}News", "\u{2003}\u{2003}Sub", "\u{2003}\u{2003}\u{2003}Deep"])
        #expect(AddFeedModel().folderId == nil, "the top level is the default")
        #expect(FolderChoice.choices(from: []).map(\.title) == ["Feeds"])
    }

    @Test func a_folder_that_disappears_resets_the_choice_to_the_top_level() {
        let model = AddFeedModel()
        let choices = [FolderChoice(folderId: nil, name: "Feeds", depth: 0), FolderChoice(folderId: 3, name: "A", depth: 1)]
        model.folderId = 3
        model.keepFolder(in: choices)
        #expect(model.folderId == 3)
        model.keepFolder(in: [choices[0]])
        #expect(model.folderId == nil)
    }

    @Test func the_chosen_folder_reaches_the_cli() async throws {
        let (state, _, stub) = try makeState(stub: Self.added + "\necho \"$*\" >> \"$(dirname \"$0\")/calls\"")
        let model = AddFeedModel()
        model.address = "df.example"
        model.folderId = 3
        #expect(await model.add(to: state))
        #expect(try calls(stub).last?.hasSuffix("add df.example --folder 3") == true)
    }

    @Test func added_event_decodes() {
        let line = #"{"event":"added","id":7,"title":"T","xml_url":"https://a.example/f","page":120,"theme":"Tech","territory":null,"discovered":false,"like":"Sibling"}"#
        #expect(EngineEvent.decode(line: line)
            == .added(AddedFeed(id: 7, title: "T", theme: "Tech", like: "Sibling")))
        let bare = #"{"event":"added","id":8,"title":"U","xml_url":"https://a.example/g","page":121,"theme":null,"territory":null,"discovered":true}"#
        #expect(EngineEvent.decode(line: bare) == .added(AddedFeed(id: 8, title: "U", theme: nil, like: nil)))
        let inFolder = #"{"event":"added","id":9,"title":"V","xml_url":"https://a.example/h","page":201,"theme":null,"territory":null,"folder":3,"discovered":false,"like":null}"#
        #expect(EngineEvent.decode(line: inFolder) == .added(AddedFeed(id: 9, title: "V", theme: nil, like: nil, folder: 3)))
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

    /// An add during a refresh is refused by the app itself: a sentence, not a failure to log.
    @Test func a_busy_refusal_shows_its_sentence_without_the_log_button() async throws {
        let (state, _, _) = try makeState(stub: """
            case "$*" in *fetch*) sleep 1 ;; esac
            """)
        let running = state.runs.start(.fetch(feeds: []))
        #expect(state.runs.isLongRunActive)
        let model = AddFeedModel()
        model.address = "https://df.example"
        #expect(await model.add(to: state) == false)
        #expect(model.errorMessage == "A refresh is running. Try again when it ends.")
        #expect(!model.showsLog)
        _ = await running.value
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
        for line in ["import /Users/willow/feeds.opml --dry-run", "export --output /tmp/a.b/c.opml",
                     "feed theme 7 ai-labs", "feed move 12 root", "add localhost",
                     "add [::1]:8080/feed", "add héhé.example/flux"] {
            #expect(LogRedaction.redact(line) == line)
        }
    }

    @Test func scheme_less_addresses_are_redacted() {
        #expect(LogRedaction.redact("add example.com/f.xml?token=abc") == "add example.com/f.xml?…")
        #expect(LogRedaction.redact("add u:p@example.com/feed") == "add example.com/feed")
        #expect(LogRedaction.redact("add www.example.org#frag") == "add www.example.org")
        #expect(LogRedaction.redact("add localhost:8080?k=v") == "add localhost:8080?…")
    }

    @Test func userinfo_goes_before_the_query_cut() {
        #expect(LogRedaction.redact("add https://u:pa?ss@host.example/f") == "add https://host.example/f")
        #expect(LogRedaction.redact("add u:pa#ss@host.example/f") == "add host.example/f")
    }

    @Test func the_address_add_takes_is_redacted_whatever_it_looks_like() {
        #expect(LogRedaction.commandLine(["add", "intranet?token=abc"]) == "add intranet?…")
        #expect(LogRedaction.commandLine(["add", "u:p@[::1]:8080/feed", "--theme", "tech"]) == "add [::1]:8080/feed --theme tech")
        #expect(LogRedaction.commandLine(["add", "nas/feed xml?token=abc"]) == "add nas/feed xml?…")
        #expect(LogRedaction.commandLine(RunRequest.addFeed(address: "intranet#k", theme: nil).arguments) == "add intranet")
    }

    @Test func other_command_lines_go_through_the_word_rule() {
        #expect(LogRedaction.commandLine(["import", "/Users/willow/feeds.opml", "--dry-run"])
            == "import /Users/willow/feeds.opml --dry-run")
        #expect(LogRedaction.commandLine(["feed", "move", "12", "root"]) == "feed move 12 root")
        #expect(LogRedaction.commandLine(["extract", "https://u:p@host.example/a?b=c"]) == "extract https://host.example/a?…")
    }
}
