import AppKit
import SwiftUI
import UniformTypeIdentifiers

extension UTType {
    static let opml = UTType(filenameExtension: "opml", conformingTo: .xml) ?? .xml
}

/// The JSON line `ressac --json import [--dry-run]` prints (core `ImportReport`).
struct ImportReport: Decodable, Sendable, Equatable {
    let dryRun: Bool
    let foldersAdded: Int
    let feedsAdded: Int
    /// `[dropped title, kept title]` for duplicates inside the file.
    let merged: [[String]]
    let skippedExisting: Int
}

struct ImportPreviewFailure: Error, Equatable {
    let exitCode: Int32
    let message: String
}

/// Runs `ressac import --dry-run` for the confirmation sheet. Its JSON line carries no `event`
/// tag, so it does not go through `RessacCLI`'s event stream; it is read-only and short.
enum ImportPreview {
    static func arguments(database: URL, file: URL) -> [String] {
        ["--db", database.path(percentEncoded: false), "--json", "import", "--dry-run", file.path(percentEncoded: false)]
    }

    static func decode(_ output: Data) throws -> ImportReport {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard let line = output.split(separator: UInt8(ascii: "\n")).last(where: { $0.first == UInt8(ascii: "{") }) else {
            throw ImportPreviewFailure(exitCode: 0, message: "The import preview printed nothing.")
        }
        return try decoder.decode(ImportReport.self, from: Data(line))
    }

    static func run(executable: URL, database: URL, file: URL) async throws -> ImportReport {
        let process = Process()
        process.executableURL = executable
        process.arguments = arguments(database: database, file: file)
        let stdout = Pipe()
        let stderr = Pipe()
        process.standardOutput = stdout
        process.standardError = stderr
        let (exits, exitContinuation) = AsyncStream.makeStream(of: Int32.self)
        process.terminationHandler = { finished in
            let code = finished.terminationReason == .uncaughtSignal ? -finished.terminationStatus : finished.terminationStatus
            exitContinuation.yield(code)
            exitContinuation.finish()
        }
        try process.run()
        let outHandle = stdout.fileHandleForReading
        let errHandle = stderr.fileHandleForReading
        // Drain both pipes concurrently so a chatty stderr can never block the child.
        async let output = Task.detached { outHandle.readDataToEndOfFile() }.value
        async let errors = Task.detached { errHandle.readDataToEndOfFile() }.value
        let (outData, errData) = await (output, errors)
        var status: Int32 = 0
        for await code in exits { status = code }
        guard status == 0 else {
            let last = String(decoding: errData, as: UTF8.self).split(whereSeparator: \.isNewline).last.map(String.init)
            let message = last.map { $0.hasPrefix("error: ") ? String($0.dropFirst(7)) : $0 } ?? "The import preview failed."
            throw ImportPreviewFailure(exitCode: status, message: message)
        }
        return try decode(outData)
    }
}

/// Import (drop, ⌘O, first run) and export (⇧⌘E) of OPML files. Phase 9's File menu
/// sets `AppState.pendingPanel`; `libraryChrome` turns it into `choose()` or the save panel.
@MainActor @Observable final class OPMLFlow {
    enum Step: Equatable {
        case idle
        case previewing(URL)
        case confirming(URL, ImportReport)
        case importing(URL)
        case failed(String)
    }

    private(set) var step: Step = .idle
    var isChoosingFile = false

    @ObservationIgnored private let preview: @Sendable (URL) async throws -> ImportReport
    @ObservationIgnored private let submit: @MainActor (RunRequest) async -> Bool
    @ObservationIgnored private let isRefreshRunning: @MainActor () -> Bool

    init(preview: @escaping @Sendable (URL) async throws -> ImportReport,
         submit: @escaping @MainActor (RunRequest) async -> Bool,
         isRefreshRunning: @escaping @MainActor () -> Bool = { false }) {
        self.preview = preview
        self.submit = submit
        self.isRefreshRunning = isRefreshRunning
    }

    static let refreshRunningMessage = "A refresh is running; try again when it ends."

    /// The coordinator refuses an import while a long run is active, so the sheet says so
    /// instead of closing with nothing imported.
    var importBlockedMessage: String? { isRefreshRunning() ? Self.refreshRunningMessage : nil }

    /// The bundled helper on the database in use, and the app's run coordinator.
    static func live(database: URL, runs: RunCoordinator) -> OPMLFlow {
        OPMLFlow(
            preview: { file in
                guard let executable = RessacCLI.bundledExecutable() else {
                    throw ImportPreviewFailure(exitCode: 1, message: "The ressac helper is missing from the app.")
                }
                return try await ImportPreview.run(executable: executable, database: database, file: file)
            },
            submit: { await runs.submit($0) },
            isRefreshRunning: { runs.isLongRunActive }
        )
    }

    var isBusy: Bool {
        switch step {
        case .previewing, .importing: true
        case .idle, .confirming, .failed: false
        }
    }

    var failureMessage: String? {
        if case .failed(let message) = step { message } else { nil }
    }

    var pendingReport: (file: URL, report: ImportReport)? {
        if case .confirming(let file, let report) = step { (file, report) } else { nil }
    }

    func choose() {
        guard !isBusy else { return }
        isChoosingFile = true
    }

    static func firstOPML(in urls: [URL]) -> URL? {
        urls.first { $0.isFileURL && $0.pathExtension.lowercased() == "opml" }
    }

    /// Previews the first `.opml` among `urls` with `--dry-run`. Returns `false` when there is none.
    @discardableResult
    func open(_ urls: [URL]) async -> Bool {
        guard !isBusy, let file = Self.firstOPML(in: urls) else { return false }
        step = .previewing(file)
        do {
            step = .confirming(file, try await preview(file))
        } catch let failure as ImportPreviewFailure {
            step = .failed(failure.message)
        } catch {
            step = .failed(error.localizedDescription)
        }
        return true
    }

    /// Imports the confirmed file, then fetches every feed (spec §7, first run).
    func confirm() async {
        guard case .confirming(let file, _) = step, importBlockedMessage == nil else { return }
        step = .importing(file)
        if await submit(.importOPML(file, dryRun: false)) {
            _ = await submit(.fetch(feeds: []))
        }
        step = .idle
    }

    func cancel() {
        guard !isBusy else { return }
        step = .idle
    }

    @discardableResult
    func export(to url: URL) async -> Bool {
        await submit(.export(to: url, cemetery: false))
    }

    /// Asks where to save, as a sheet on the key window, then runs `ressac export -o`.
    func chooseExportDestination() async {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = "Ressac.opml"
        panel.allowedContentTypes = [.opml]
        panel.canCreateDirectories = true
        let response = if let window = NSApp.keyWindow {
            await panel.beginSheetModal(for: window)
        } else {
            await panel.begin()
        }
        guard response == .OK, let url = panel.url else { return }
        await export(to: url)
    }

    /// The confirmation sheet's lines, from the dry-run counts.
    static func summary(of report: ImportReport) -> [String] {
        var lines: [String] = []
        if report.feedsAdded == 0 {
            lines.append("No new feeds in this file.")
        } else {
            lines.append("\(count(report.feedsAdded, "feed")) in \(count(report.foldersAdded, "new folder")).")
        }
        if !report.merged.isEmpty {
            lines.append("\(count(report.merged.count, "duplicate")) inside the file will be merged:")
            lines += report.merged.compactMap { pair in
                pair.count == 2 ? "“\(pair[0])” into “\(pair[1])”" : nil
            }
        }
        if report.skippedExisting > 0 {
            lines.append("\(count(report.skippedExisting, "feed")) already in Ressac will be skipped.")
        }
        return lines
    }

    private static func count(_ n: Int, _ noun: String) -> String {
        "\(n) \(noun)\(n == 1 ? "" : "s")"
    }
}

extension View {
    /// First-run screen, OPML drop target, import sheet, file chooser and export panel for
    /// the main window.
    func libraryChrome(_ state: AppState) -> some View {
        modifier(LibraryChrome(state: state))
    }
}

private struct LibraryChrome: ViewModifier {
    let state: AppState
    @State private var flow: OPMLFlow
    @State private var isDropTargeted = false

    init(state: AppState) {
        self.state = state
        _flow = State(initialValue: OPMLFlow.live(database: DatabaseLocation.current(), runs: state.runs))
    }

    func body(content: Content) -> some View {
        Group {
            // `sidebar` is observed with immediate scheduling (Phase 9), so an existing
            // library never flashes the first-run screen.
            if state.sidebar.feeds.isEmpty {
                FirstRunView(flow: flow)
            } else {
                content
            }
        }
        .dropDestination(for: URL.self) { urls, _ in
            guard OPMLFlow.firstOPML(in: urls) != nil, !flow.isBusy else { return false }
            Task { await flow.open(urls) }
            return true
        } isTargeted: { isDropTargeted = $0 }
        .overlay {
            if isDropTargeted {
                RoundedRectangle(cornerRadius: 10)
                    .strokeBorder(Color.accentColor, style: StrokeStyle(lineWidth: 3, dash: [8, 6]))
                    .background(Color.accentColor.opacity(0.05), in: RoundedRectangle(cornerRadius: 10))
                    .padding(6)
                    .allowsHitTesting(false)
                    .transition(.opacity)
            }
        }
        .animation(.easeOut(duration: 0.15), value: isDropTargeted)
        .sheet(isPresented: Binding(get: { flow.pendingReport != nil }, set: { if !$0 { flow.cancel() } })) {
            if let pending = flow.pendingReport {
                ImportConfirmationSheet(file: pending.file, report: pending.report, flow: flow)
            }
        }
        .sheet(isPresented: Binding(get: { state.pendingSheet == .addFeed }, set: { if !$0 { state.pendingSheet = nil } })) {
            AddFeedSheet(state: state)
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if let note = state.addFeedNote {
                AddFeedNoteView(note: note) { state.addFeedNote = nil }
            }
        }
        .alert("This OPML file can't be imported",
               isPresented: Binding(get: { flow.failureMessage != nil }, set: { if !$0 { flow.cancel() } })) {
            Button("OK", role: .cancel) { flow.cancel() }
        } message: {
            Text(flow.failureMessage ?? "")
        }
        .fileImporter(isPresented: $flow.isChoosingFile, allowedContentTypes: [.opml]) { result in
            guard case .success(let url) = result else { return }
            Task { await flow.open([url]) }
        }
        .onChange(of: state.pendingPanel, initial: true) { _, panel in
            guard let panel else { return }
            state.pendingPanel = nil
            switch panel {
            case .importOPML: flow.choose()
            case .exportOPML: Task { await flow.chooseExportDestination() }
            }
        }
    }
}

/// "No database, or no feeds" (spec §7): a centred empty state with a button.
struct FirstRunView: View {
    let flow: OPMLFlow

    var body: some View {
        ContentUnavailableView {
            Label("No Feeds Yet", systemImage: "dot.radiowaves.up.forward")
        } description: {
            Text("Drop an OPML file here, or choose one…")
        } actions: {
            Button("Choose OPML File…") { flow.choose() }
                .keyboardShortcut(.defaultAction)
                .disabled(flow.isBusy)
            if flow.isBusy {
                ProgressView().controlSize(.small)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// Shows the `--dry-run` counts before anything is written.
struct ImportConfirmationSheet: View {
    let file: URL
    let report: ImportReport
    let flow: OPMLFlow

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Label("Import “\(file.lastPathComponent)”?", systemImage: "tray.and.arrow.down")
                .font(.headline)
            VStack(alignment: .leading, spacing: 4) {
                ForEach(Array(OPMLFlow.summary(of: report).enumerated()), id: \.offset) { _, line in
                    Text(line)
                }
            }
            .textSelection(.enabled)
            if report.feedsAdded > 0 {
                Text("Ressac fetches every feed right after the import.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            if let blocked = flow.importBlockedMessage {
                Text(blocked)
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            HStack {
                Spacer()
                Button("Cancel", role: .cancel) { flow.cancel() }
                    .keyboardShortcut(.cancelAction)
                Button("Import") { Task { await flow.confirm() } }
                    .keyboardShortcut(.defaultAction)
                    .disabled(report.feedsAdded == 0 || flow.importBlockedMessage != nil)
            }
        }
        .padding(20)
        .frame(width: 420)
    }
}
