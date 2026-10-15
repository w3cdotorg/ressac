import AppKit
import Synchronization
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
                PageHeader(width: width, windowSize: proxy.size)
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
                    titled(.reader, width: max(0, width - Self.minimumCells + Self.minimumReaderCells),
                           blankWhenInactive: antiope.readerPage != .article) { readerColumn }
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

    /// A column under its one-row title (blank while inactive on P100 and P899, which have their own heading).
    private func titled(_ column: AntiopeColumn, width: Int, blankWhenInactive: Bool = false, @ViewBuilder content: () -> some View) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            ColumnTitleView(column: column, width: width, blankWhenInactive: blankWhenInactive)
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
    /// The window's size, for the tube's corner (`rightMarginCells`).
    let windowSize: CGSize

    @AppStorage(Appearance.Keys.appearance) private var appearance = Appearance.native
    @AppStorage(Appearance.Keys.crt) private var crt = false
    @AppStorage(ReaderStyle.Keys.antiopeSize) private var storedSize = TeletextFont.defaultSize
    @Environment(AppState.self) private var app
    @Environment(AntiopeState.self) private var antiope
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        // The counter is decorative and stays off with Reduce Motion (spec §8, Accessibility).
        let rolling = app.runs.isLongRunActive && !reduceMotion
        let margin = Self.rightMarginCells(
            windowSize: windowSize, cell: TeletextFont.cell(forSize: TeletextFont.snapped(storedSize)),
            crt: CRTGeometry.isActive(
                appearance: Appearance.effective(stored: appearance, fontAvailable: TeletextFont.isAvailable), crt: crt))
        TimelineView(.periodic(from: .now, by: rolling ? 0.1 : 1)) { context in
            CellLineView(line: Self.line(
                label: Self.label(entry: antiope.entry, notFound: antiope.notFound, current: antiope.currentPage,
                                  rolling: rolling ? Self.rollingPage(at: context.date) : nil),
                date: context.date, width: width, rightMargin: margin))
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

    /// Blank cells kept at the right of the header. The CRT screen paints the tube's corner over
    /// the top-right of the window (a radius of 3 % of the shorter side, `CRTGeometry.cornerRadius`),
    /// and the bleed draws up to `bleedOffset + glowRadius` beyond a glyph: the last cells of the
    /// time sat under that mask. With the screen on the margin covers both, rounded up to whole
    /// cells; with it off nothing covers the corner, so the time stays flush with the edge.
    static func rightMarginCells(windowSize: CGSize, cell: CGSize, crt: Bool) -> Int {
        guard crt, cell.width > 0 else { return 0 }
        let reach = CRTGeometry.cornerRadius(for: windowSize) + CRTGeometry.bleedOffset + CRTGeometry.glowRadius
        return Int((reach / cell.width).rounded(.up))
    }

    /// The page in yellow (red when not found) and `RESSAC` in white on the left; the date in cyan
    /// and the time in yellow at the right edge. Exactly `width` cells: when the window is narrow,
    /// the time keeps its cells first, then the date, then the page. `rightMargin` blank cells
    /// close the row, inside `width` (never more than a quarter of it, and never at the time's expense).
    static func line(label: HeaderLabel, date: Date, width: Int, rightMargin: Int = 0,
                     timeZone: TimeZone = .current) -> CellLine {
        let day = TeletextDateFormat.string(from: date, pattern: "EEE dd MMM", timeZone: timeZone).uppercased() + " "
        let time = TeletextDateFormat.string(from: date, pattern: "HH:mm:ss", timeZone: timeZone)
        let (page, colour, showsName): (String, TeletextColor, Bool) = switch label {
        case let .page(number): ("P" + (number.map { String($0) } ?? "---"), .yellow, true)
        case let .typing(digits): ("P" + digits, .yellow, true)
        case let .searching(number): ("P\(number)", .yellow, true)
        case let .notFound(number): ("P\(number) NOT FOUND", .red, false)
        }
        let margin = max(0, min(rightMargin, width / 4, width - time.count))
        var remaining = max(0, width) - margin
        func take(_ wanted: Int) -> Int {
            let cells = min(wanted, remaining)
            remaining -= cells
            return cells
        }
        let timeCells = take(time.count)
        let dayCells = take(day.count)
        let pageCells = take(page.count + 1)
        let nameCells = take(remaining)
        var spans = [
            CellSpan(page, cells: pageCells, foreground: colour),
            CellSpan(showsName ? "RESSAC" : "", cells: nameCells, foreground: .white),
            CellSpan(day, cells: dayCells, foreground: .cyan),
            CellSpan(time, cells: timeCells, foreground: .yellow),
        ]
        if margin > 0 { spans.append(CellSpan("", cells: margin, foreground: .white)) }
        return CellLine(spans)
    }
}

/// Fixed-pattern dates for the teletext pages, in English whatever the system language. One
/// `DateFormatter` per pattern and time zone, kept: the header formats the date and the time every
/// second (every tenth of a second while the counter rolls), and building a `DateFormatter` costs
/// about a hundred times more than using one.
enum TeletextDateFormat {
    private struct Key: Hashable {
        var pattern: String
        var timeZone: TimeZone
    }

    /// A handful of keys in practice: three patterns, times the zones the app has run in.
    private static let formatters = Mutex<[Key: DateFormatter]>([:])

    static func string(from date: Date, pattern: String, timeZone: TimeZone) -> String {
        formatters.withLock { cache in
            let key = Key(pattern: pattern, timeZone: timeZone)
            if let formatter = cache[key] { return formatter.string(from: date) }
            let formatter = DateFormatter()
            formatter.locale = Locale(identifier: "en_US_POSIX")
            formatter.calendar = Calendar(identifier: .gregorian)
            formatter.timeZone = timeZone
            formatter.dateFormat = pattern
            cache[key] = formatter
            return formatter.string(from: date)
        }
    }

    /// Whether `pattern` in `timeZone` already has its formatter (for the tests).
    static func isCached(_ pattern: String, timeZone: TimeZone) -> Bool {
        formatters.withLock { $0[Key(pattern: pattern, timeZone: timeZone)] != nil }
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
