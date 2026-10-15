import AppKit
import SwiftUI

/// The Antiope look of the reading window (spec §5, Window): the page header, the index, entry and
/// reader columns between straight separators, and the Minitel bar, on black. It shares `AppState`
/// with `MainView`; `AntiopeState` holds what only this look needs, for as long as it is on screen.
struct AntiopeRootView: View {
    /// The narrowest reader column, in cells, so that the window never squeezes it away.
    static let minimumReaderCells = 24
    /// Index, separator, entries, separator, reader.
    static let minimumCells = PageIndexColumn.cells + 1 + EntryPageColumn.cells + 1 + minimumReaderCells

    let state: AppState
    @State private var antiope: AntiopeState
    /// Shared by the garden pages and the Minitel bar (phase 12).
    @State private var garden = GardenStore()
    /// Antiope's own text size (decision of 2026-10-14), separate from the Native `reader.size`.
    @AppStorage(ReaderStyle.Keys.antiopeSize) private var storedSize = TeletextFont.defaultSize

    init(state: AppState) {
        self.state = state
        _antiope = State(initialValue: AntiopeState(app: state))
    }

    var body: some View {
        let size = TeletextFont.snapped(storedSize)
        let cell = TeletextFont.cell(forSize: size)
        GeometryReader { proxy in
            let width = max(0, Int(proxy.size.width / cell.width))
            VStack(alignment: .leading, spacing: 0) {
                PageHeader(width: width)
                if let banner = state.runs.banner {
                    AntiopeBannerRow(banner: banner) { state.runs.banner = nil }
                }
                HStack(alignment: .top, spacing: 0) {
                    titled(.index, width: PageIndexColumn.cells) { PageIndexColumn() }
                        .frame(width: CGFloat(PageIndexColumn.cells) * cell.width)
                    ColumnSeparator()
                    titled(.entries, width: EntryPageColumn.cells) { entriesColumn }
                        .frame(width: CGFloat(EntryPageColumn.cells) * cell.width)
                    ColumnSeparator()
                    titled(.reader, width: max(0, width - Self.minimumCells + Self.minimumReaderCells)) { readerColumn }
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                }
                .frame(maxHeight: .infinity, alignment: .top)
                MinitelBar(width: width)
            }
        }
        .frame(minWidth: CGFloat(Self.minimumCells) * cell.width)
        .background(TeletextColor.black.color.ignoresSafeArea())
        .toolbar(.hidden, for: .windowToolbar)
        .background(ReadingWindowReader { [state, antiope] in
            ReadingKeyRouter(state: state, focus: antiope, antiope: antiope)
        })
        .background(AntiopeWindowChrome())
        .environment(state)
        .environment(antiope)
        .environment(garden)
        .environment(\.teletextSize, size)
        // Neither look inherits an open search prompt; the search text stays (Review Focus #5).
        .onAppear { antiope.closeSearchPrompt(clearing: false) }
        .onDisappear { antiope.closeSearchPrompt(clearing: false) }
        .crtScreen()
    }

    /// A column under its one-row title.
    private func titled(_ column: AntiopeColumn, width: Int, @ViewBuilder content: () -> some View) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            ColumnTitleView(column: column, width: width)
            content()
        }
    }

    /// The entry list, or P800–P802 in a Garden scope.
    @ViewBuilder private var entriesColumn: some View {
        if case let .garden(scope) = state.selection {
            GardenEntryPage(scope: scope)
        } else {
            EntryPageColumn()
        }
    }

    /// P100 SOMMAIRE, P899 GUIDE, or else the selected entry (`ReaderView`, themed by phase 10) or,
    /// in a Garden scope, the selected feed's inspector page.
    @ViewBuilder private var readerColumn: some View {
        switch antiope.readerPage {
        case .article:
            if state.selection.isGarden {
                GardenInspectorPage()
            } else {
                ReaderView(state: state)
            }
        case .index:
            SommairePage()
        case .guide:
            GuidePage()
        }
    }
}

/// What the header shows in place of the current page (spec §6, Page header).
enum HeaderLabel: Equatable, Sendable {
    /// The page on screen; nil while the selection has no number yet (`P---`).
    case page(Int?)
    /// The digits being typed, `"4__"`.
    case typing(String)
    /// An unknown page, in red for two seconds.
    case notFound(Int)
    /// The counter rolling during a refresh, as a TV searches for a page.
    case searching(Int)
}

/// The top row: `P412 RESSAC` … `WED 14 OCT 09:47:12`, ticking every second.
struct PageHeader: View {
    let width: Int

    @Environment(AppState.self) private var app
    @Environment(AntiopeState.self) private var antiope
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        // The counter is decorative and stays off with Reduce Motion (spec §8, Accessibility).
        let rolling = app.runs.isLongRunActive && !reduceMotion
        TimelineView(.periodic(from: .now, by: rolling ? 0.1 : 1)) { context in
            CellLineView(line: Self.line(
                label: Self.label(entry: antiope.entry, notFound: antiope.notFound, current: antiope.currentPage,
                                  rolling: rolling ? Self.rollingPage(at: context.date) : nil),
                date: context.date, width: width))
        }
    }

    /// Typing first, then `NOT FOUND`, then the rolling counter, then the page.
    static func label(entry: PageEntry, notFound: Int?, current: Int?, rolling: Int?) -> HeaderLabel {
        if entry.isActive { return .typing(entry.display) }
        if let notFound { return .notFound(notFound) }
        if let rolling { return .searching(rolling) }
        return .page(current)
    }

    /// 100…899, one step every tenth of a second.
    static func rollingPage(at date: Date) -> Int {
        let tenths = Int((date.timeIntervalSinceReferenceDate * 10).rounded(.down))
        return 100 + ((tenths % 800) + 800) % 800
    }

    /// The page in yellow (red when not found) and `RESSAC` in white on the left; the date in cyan
    /// and the time in yellow at the right edge. Exactly `width` cells: when the window is narrow,
    /// the time keeps its cells first, then the date, then the page.
    static func line(label: HeaderLabel, date: Date, width: Int, timeZone: TimeZone = .current) -> CellLine {
        let day = format(date, "EEE dd MMM", timeZone: timeZone).uppercased() + " "
        let time = format(date, "HH:mm:ss", timeZone: timeZone)
        let (page, colour, showsName): (String, TeletextColor, Bool) = switch label {
        case let .page(number): ("P" + (number.map { String($0) } ?? "---"), .yellow, true)
        case let .typing(digits): ("P" + digits, .yellow, true)
        case let .searching(number): ("P\(number)", .yellow, true)
        case let .notFound(number): ("P\(number) NOT FOUND", .red, false)
        }
        var remaining = max(0, width)
        func take(_ wanted: Int) -> Int {
            let cells = min(wanted, remaining)
            remaining -= cells
            return cells
        }
        let timeCells = take(time.count)
        let dayCells = take(day.count)
        let pageCells = take(page.count + 1)
        let nameCells = take(remaining)
        return CellLine([
            CellSpan(page, cells: pageCells, foreground: colour),
            CellSpan(showsName ? "RESSAC" : "", cells: nameCells, foreground: .white),
            CellSpan(day, cells: dayCells, foreground: .cyan),
            CellSpan(time, cells: timeCells, foreground: .yellow),
        ])
    }

    private static func format(_ date: Date, _ pattern: String, timeZone: TimeZone) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.timeZone = timeZone
        formatter.dateFormat = pattern
        return formatter.string(from: date)
    }
}

/// One cell drawing a blue `▐` on every row (spec §5): the right half of the cell in blue, the full
/// height of the columns. A shape, never text, so no glyph can bend it.
struct ColumnSeparator: View {
    @Environment(\.teletextSize) private var size

    var body: some View {
        let cell = TeletextFont.cell(forSize: size)
        TeletextColor.blue.color
            .frame(width: cell.width / 2)
            .frame(width: cell.width, alignment: .trailing)
            .frame(maxHeight: .infinity)
            .accessibilityHidden(true)
    }
}

/// Dresses the window for Antiope while the root view is in it: no title, a transparent title bar
/// over full-size black content, dark controls. It puts everything back when it leaves, so a live
/// switch to Native finds the window as it was.
struct AntiopeWindowChrome: NSViewRepresentable {
    func makeNSView(context: Context) -> ChromeView { ChromeView() }

    func updateNSView(_ view: ChromeView, context: Context) {}

    final class ChromeView: NSView {
        private struct Saved {
            var titleVisibility: NSWindow.TitleVisibility
            var titlebarAppearsTransparent: Bool
            var fullSizeContent: Bool
            var backgroundColor: NSColor
            var appearance: NSAppearance?
        }

        private var saved: Saved?
        private weak var dressed: NSWindow?

        override func viewWillMove(toWindow newWindow: NSWindow?) {
            super.viewWillMove(toWindow: newWindow)
            if let dressed, newWindow !== dressed { undress(dressed) }
        }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if let window, dressed !== window { dress(window) }
        }

        private func dress(_ window: NSWindow) {
            saved = Saved(titleVisibility: window.titleVisibility,
                          titlebarAppearsTransparent: window.titlebarAppearsTransparent,
                          fullSizeContent: window.styleMask.contains(.fullSizeContentView),
                          backgroundColor: window.backgroundColor, appearance: window.appearance)
            window.titleVisibility = .hidden
            window.titlebarAppearsTransparent = true
            window.styleMask.insert(.fullSizeContentView)
            window.backgroundColor = .black
            window.appearance = NSAppearance(named: .darkAqua)
            dressed = window
        }

        private func undress(_ window: NSWindow) {
            defer {
                saved = nil
                dressed = nil
            }
            guard let saved else { return }
            window.titleVisibility = saved.titleVisibility
            window.titlebarAppearsTransparent = saved.titlebarAppearsTransparent
            if !saved.fullSizeContent { window.styleMask.remove(.fullSizeContentView) }
            window.backgroundColor = saved.backgroundColor
            window.appearance = saved.appearance
        }
    }
}
