import AppKit
import SwiftUI

/// What the Sea Chart window does with the page's requests (spec §8).
@MainActor enum ChartActions {
    static func perform(_ action: ChartAction, state: AppState, defaults: UserDefaults = .standard,
                        reduceMotion: Bool = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion) {
        switch action {
        case let .openFeed(id):
            state.selection = .feed(id)
            state.showReadingWindow()
        case let .openURL(url):
            state.openURL(url)
        case let .prefs(prefs):
            prefs.save(to: defaults, reduceMotion: reduceMotion)
        }
    }
}

/// The Sea Chart window's content (spec §8): the chart of the open database, or why there is none.
struct ChartWindow: View {
    static let id = "sea-chart"

    let launcher: Launcher

    var body: some View {
        Group {
            if let state = launcher.state {
                // A new database (Use Default Database) is a new chart.
                ChartScreen(state: state).id(ObjectIdentifier(state))
            } else if launcher.offersDefaultDatabase {
                Text("The database is not open. The main window says why.")
                    .foregroundStyle(.secondary)
                    .padding(40)
            } else {
                ProgressView()
            }
        }
        .frame(minWidth: 640, minHeight: 360)
        .task {
            // The window can be restored alone at launch; the launch runs once whichever window asks.
            guard !Launcher.isRunningTests else { return }
            await launcher.launch(database: DatabaseLocation.current(), executable: RessacCLI.bundledExecutable())
        }
    }
}

/// The chart while the window is open: drawn on open, redrawn after each database change.
private struct ChartScreen: View {
    let state: AppState
    @State private var model: ChartModel
    @Environment(\.openWindow) private var openWindow

    init(state: AppState) {
        self.state = state
        _model = State(initialValue: ChartModel(runner: CLIChartRunner(cli: state.runs.cli)))
    }

    var body: some View {
        content
            // Cancelled when the window closes, which cancels the model's work too.
            .task { await model.follow { [state] in state.changeTick } }
            // Keeps the chart live behind another app: the database is polled while it is on screen.
            .background(ChartVisibilityReader { [state] in state.isChartOnScreen = $0 })
    }

    @ViewBuilder private var content: some View {
        switch model.phase {
        case .loading:
            ProgressView()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        case let .failed(message):
            failure(message)
        case let .ready(json):
            ChartWebView(json: json, prefs: Self.pagePrefs, onAction: perform)
                .overlay(alignment: .bottom) {
                    // A failed refresh keeps the last chart; the failure stays visible here.
                    if let message = model.lastRefreshError {
                        HStack(spacing: 12) {
                            Text(message).lineLimit(2)
                            Button("Retry") { model.refresh() }
                        }
                        .padding(.horizontal, 14)
                        .padding(.vertical, 8)
                        .background(.regularMaterial, in: .capsule)
                        .padding(16)
                    }
                }
        }
    }

    private func failure(_ message: String) -> some View {
        VStack(spacing: 12) {
            Text("The chart could not be drawn").font(.title3)
            Text(message)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .textSelection(.enabled)
            Button("Retry") { model.refresh() }
        }
        .padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func perform(_ action: ChartAction) {
        ChartActions.perform(action, state: state)
        // The main window was closed: open one, which shows the feed just selected.
        if case .openFeed = action, !state.hasReadingWindow {
            openWindow(id: RessacApp.mainWindowID)
        }
    }

    private static func pagePrefs() -> [String: Any] {
        ChartPrefs.load(from: .standard)
            .pageArgument(reduceMotion: NSWorkspace.shared.accessibilityDisplayShouldReduceMotion)
    }
}

/// Tells `AppState` whether the Sea Chart's window is at least partly visible (its occlusion
/// state), so that terminal writes are still polled for while the chart sits on a second display
/// behind another app. Leaving the window (closing it) reports "not on screen".
struct ChartVisibilityReader: NSViewRepresentable {
    let onChange: @MainActor (Bool) -> Void

    func makeNSView(context: Context) -> WindowView { WindowView(onChange: onChange) }

    func updateNSView(_ view: WindowView, context: Context) {}

    static func dismantleNSView(_ view: WindowView, coordinator: ()) {
        view.leave()
    }

    final class WindowView: NSView {
        private let onChange: @MainActor (Bool) -> Void
        /// The window whose occlusion changes are followed, until `leave()`.
        private weak var watched: NSWindow?
        private var observer: (any NSObjectProtocol)?

        init(onChange: @escaping @MainActor (Bool) -> Void) {
            self.onChange = onChange
            super.init(frame: .zero)
        }

        @available(*, unavailable)
        required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

        override func viewWillMove(toWindow newWindow: NSWindow?) {
            super.viewWillMove(toWindow: newWindow)
            if newWindow !== window { leave() }
        }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            guard let window, window !== watched else { return }
            watched = window
            observer = NotificationCenter.default.addObserver(
                forName: NSWindow.didChangeOcclusionStateNotification, object: window, queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated { self?.report() }
            }
            report()
        }

        private func report() {
            guard let watched else { return }
            onChange(watched.occlusionState.contains(.visible))
        }

        /// Stops following the window and reports "not on screen", once.
        func leave() {
            guard observer != nil else { return }
            if let observer { NotificationCenter.default.removeObserver(observer) }
            observer = nil
            watched = nil
            onChange(false)
        }
    }
}
