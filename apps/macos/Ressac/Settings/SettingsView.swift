import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// Where the database lives: `RESSAC_DB`, else the Settings choice, else the core's default
/// (shared with the `ressac` command). The app reads it once at launch.
enum DatabaseLocation {
    /// `UserDefaults` key of the Settings choice; empty or missing means the default.
    static let defaultsKey = "databasePath"

    /// `$HOME/Library/Application Support/Ressac/ressac.sqlite` (Phase 7, without `RESSAC_DB`).
    static var defaultURL: URL { AppDatabase.defaultURL(environment: [:]) }

    /// Where the bundled `ressac` writes its daily logs.
    static var logsURL: URL {
        URL.homeDirectory.appending(path: "Library/Logs/Ressac", directoryHint: .isDirectory)
    }

    static func resolve(_ path: String?) -> URL {
        guard let path, !path.trimmingCharacters(in: .whitespaces).isEmpty else { return defaultURL }
        return URL(filePath: (path as NSString).expandingTildeInPath)
    }

    static func current(environment: [String: String] = ProcessInfo.processInfo.environment,
                        defaults: UserDefaults = .standard) -> URL {
        if let path = environment["RESSAC_DB"], !path.isEmpty {
            return URL(filePath: path)
        }
        return resolve(defaults.string(forKey: defaultsKey))
    }

    /// Why `url` cannot be the database, or nil when launch can use it: a missing or empty
    /// file is fine (launch creates it), an older schema is migrated, a newer one or a file
    /// that is not SQLite is refused.
    static func problem(with url: URL) -> String? {
        do {
            if case .tooNew = try AppDatabase.check(at: url) {
                return "This database was created by a newer Ressac."
            }
            return nil
        } catch {
            return "This file is not a usable Ressac database."
        }
    }

    /// Stores `url` as the Settings choice after checking it. Returns the refusal message
    /// (and leaves the setting unchanged), or nil when it was stored.
    @discardableResult
    static func choose(_ url: URL, defaults: UserDefaults = .standard) -> String? {
        if let problem = problem(with: url) { return problem }
        defaults.set(url.path(percentEncoded: false), forKey: defaultsKey)
        return nil
    }

    /// Clears the Settings choice, so the default location is used again.
    static func resetToDefault(defaults: UserDefaults = .standard) {
        defaults.removeObject(forKey: defaultsKey)
    }
}

extension UTType {
    static let sqliteDatabase = UTType(filenameExtension: "sqlite", conformingTo: .database) ?? .database
}

/// Settings (⌘,): Reading and Data (spec §7).
struct SettingsView: View {
    var body: some View {
        TabView {
            Tab("Reading", systemImage: "textformat") {
                ReadingSettings()
            }
            Tab("Data", systemImage: "externaldrive") {
                DataSettings()
            }
        }
        .frame(width: 540)
        .scenePadding()
    }
}

private struct ReadingSettings: View {
    @AppStorage(Appearance.Keys.appearance) private var appearance = Appearance.native
    @AppStorage(Appearance.Keys.crt) private var crt = false
    @AppStorage(ReaderStyle.Keys.font) private var font = ReaderFont.sfPro
    @AppStorage(ReaderStyle.Keys.size) private var size = ReaderStyle.defaultSize
    /// Antiope's own text size (decision of 2026-10-14), for the preview in Antiope.
    @AppStorage(ReaderStyle.Keys.antiopeSize) private var antiopeSize = TeletextFont.defaultSize
    @AppStorage(ReaderStyle.Keys.width) private var width = LineWidth.medium

    /// What the window shows: Native whenever Bedstead failed to load.
    private var shown: Appearance {
        Appearance.effective(stored: appearance, fontAvailable: TeletextFont.isAvailable)
    }

    var body: some View {
        Form {
            Picker("Appearance:", selection: Binding(get: { shown }, set: { appearance = $0 })) {
                ForEach(Appearance.allCases) { option in
                    Text(option.displayName).tag(option)
                }
            }
            .pickerStyle(.segmented)
            .disabled(!TeletextFont.isAvailable)
            if !TeletextFont.isAvailable {
                Text("Antiope needs the Bedstead font, which did not load.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            Toggle("CRT screen", isOn: $crt)
                .disabled(!CRTGeometry.isActive(appearance: shown, crt: true))
            Text("Scanlines, a vignette and color bleed over the Antiope window. Increase Contrast turns the bleed off.")
                .font(.callout)
                .foregroundStyle(.secondary)

            Picker("Font:", selection: $font) {
                ForEach(ReaderFont.allCases) { option in
                    Text(option.displayName)
                        .font(option.swiftUIFont(size: NSFont.systemFontSize))
                        .tag(option)
                }
            }
            .disabled(shown == .antiope)
            if shown == .antiope {
                Text("Bedstead in Antiope")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }

            TextSizeRow()

            Picker("Line width:", selection: $width) {
                ForEach(LineWidth.allCases) { option in
                    Text(option.displayName).tag(option)
                }
            }
            .pickerStyle(.segmented)

            LabeledContent("Preview:") {
                Text("The quick brown fox jumps over 1 234 lazy dogs. Il1 O0, « l'écart » se creuse.")
                    .font(shown == .antiope
                          ? TeletextFont.font(size: TeletextFont.snapped(antiopeSize))
                          : font.swiftUIFont(size: ReaderStyle.clampedSize(size)))
                    .lineLimit(3)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }

            Text("⌘+ and ⌘− change the text size while you read; ⌘0 restores it.")
                .font(.callout)
                .foregroundStyle(.secondary)
        }
    }
}

/// The text size row of Settings ▸ Reading: a 1 pt slider over the Native size in Native, a
/// four-step slider over Antiope's own size in Antiope (spec §5, decision of 2026-10-14). It
/// reads the appearance itself, so it does not depend on how `ReadingSettings` holds it.
private struct TextSizeRow: View {
    @AppStorage(ReaderStyle.Keys.size) private var size = ReaderStyle.defaultSize
    @AppStorage(ReaderStyle.Keys.antiopeSize) private var antiopeSize = TeletextFont.defaultSize
    @AppStorage(Appearance.Keys.appearance) private var storedAppearance = Appearance.native

    private var appearance: Appearance {
        Appearance.effective(stored: storedAppearance, fontAvailable: TeletextFont.isAvailable)
    }

    /// The size the look shown draws at.
    private var shownSize: Double {
        ReaderStyle.effectiveSize(appearance == .antiope ? antiopeSize : size, appearance: appearance)
    }

    var body: some View {
        LabeledContent("Text size:") {
            HStack(spacing: 8) {
                slider
                    .labelsHidden()
                Text("\(Int(shownSize)) pt")
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .frame(width: 40, alignment: .trailing)
            }
        }
    }

    @ViewBuilder
    private var slider: some View {
        switch appearance {
        case .native:
            Slider(value: $size, in: ReaderStyle.sizeRange, step: ReaderStyle.sizeStep) {
                Text("Text size")
            } minimumValueLabel: {
                Image(systemName: "textformat.size.smaller")
            } maximumValueLabel: {
                Image(systemName: "textformat.size.larger")
            }
        case .antiope:
            Slider(value: teletextStep, in: 0...Double(TeletextFont.sizeSteps.count - 1), step: 1) {
                Text("Text size")
            } minimumValueLabel: {
                Image(systemName: "textformat.size.smaller")
            } maximumValueLabel: {
                Image(systemName: "textformat.size.larger")
            }
        }
    }

    /// The index of Antiope's drawn size in `TeletextFont.sizeSteps`; moving it stores that step.
    private var teletextStep: Binding<Double> {
        Binding(
            get: { Double(TeletextFont.sizeSteps.firstIndex(of: ReaderStyle.effectiveSize(antiopeSize, appearance: .antiope)) ?? 0) },
            set: { index in
                let steps = TeletextFont.sizeSteps
                antiopeSize = steps[min(max(Int(index.rounded()), 0), steps.count - 1)]
            })
    }
}

private struct DataSettings: View {
    @AppStorage(DatabaseLocation.defaultsKey) private var databasePath = ""
    @State private var needsRelaunch = false
    @State private var refusal: String?

    private var databaseURL: URL { DatabaseLocation.resolve(databasePath) }

    var body: some View {
        Form {
            LabeledContent("Database:") {
                VStack(alignment: .leading, spacing: 6) {
                    Text(databaseURL.path(percentEncoded: false))
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .textSelection(.enabled)
                        .help(databaseURL.path(percentEncoded: false))
                    HStack {
                        Button("Choose…", action: chooseExisting)
                        Button("New…", action: chooseNew)
                        Button("Use Default") { update("") }
                            .disabled(databasePath.isEmpty)
                    }
                    Text(databasePath.isEmpty
                         ? "Shared with the ressac command in Terminal."
                         : "In Terminal, pass --db with this path to use the same database.")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                    if ProcessInfo.processInfo.environment["RESSAC_DB"] != nil {
                        Text("RESSAC_DB is set, so this launch uses it instead.")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                    }
                    if let refusal {
                        Label(refusal, systemImage: "exclamationmark.triangle")
                            .font(.callout)
                            .foregroundStyle(.red)
                    }
                    if needsRelaunch {
                        HStack {
                            Text("Ressac opens the new database after a relaunch.")
                                .font(.callout)
                            Button("Relaunch Now", action: relaunch)
                        }
                    }
                }
            }

            LabeledContent("Logs:") {
                Button("Reveal in Finder", action: revealLogs)
            }
        }
    }

    private func update(_ path: String) {
        refusal = nil
        guard path != databasePath else { return }
        if !path.isEmpty, let problem = DatabaseLocation.problem(with: DatabaseLocation.resolve(path)) {
            refusal = problem
            return
        }
        databasePath = path
        needsRelaunch = true
    }

    private func chooseExisting() {
        let panel = NSOpenPanel()
        panel.message = "Choose a Ressac database."
        panel.allowedContentTypes = [.sqliteDatabase]
        panel.directoryURL = databaseURL.deletingLastPathComponent()
        guard panel.runModal() == .OK, let url = panel.url else { return }
        update(url.path(percentEncoded: false))
    }

    private func chooseNew() {
        let panel = NSSavePanel()
        panel.message = "Ressac creates the database here on its next launch."
        panel.nameFieldStringValue = "ressac.sqlite"
        panel.allowedContentTypes = [.sqliteDatabase]
        panel.canCreateDirectories = true
        guard panel.runModal() == .OK, let url = panel.url else { return }
        update(url.path(percentEncoded: false))
    }

    private func relaunch() {
        let configuration = NSWorkspace.OpenConfiguration()
        configuration.createsNewApplicationInstance = true
        Task {
            _ = try? await NSWorkspace.shared.openApplication(at: Bundle.main.bundleURL, configuration: configuration)
            NSApp.terminate(nil)
        }
    }

    private func revealLogs() {
        let logs = DatabaseLocation.logsURL
        try? FileManager.default.createDirectory(at: logs, withIntermediateDirectories: true)
        NSWorkspace.shared.open(logs)
    }
}
