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
    @AppStorage(ReaderStyle.Keys.font) private var font = ReaderFont.sfPro
    @AppStorage(ReaderStyle.Keys.size) private var size = ReaderStyle.defaultSize
    @AppStorage(ReaderStyle.Keys.width) private var width = LineWidth.medium

    var body: some View {
        Form {
            Picker("Font:", selection: $font) {
                ForEach(ReaderFont.allCases) { option in
                    Text(option.displayName)
                        .font(option.swiftUIFont(size: NSFont.systemFontSize))
                        .tag(option)
                }
            }

            LabeledContent("Text size:") {
                HStack(spacing: 8) {
                    Slider(value: $size, in: ReaderStyle.sizeRange, step: ReaderStyle.sizeStep) {
                        Text("Text size")
                    } minimumValueLabel: {
                        Image(systemName: "textformat.size.smaller")
                    } maximumValueLabel: {
                        Image(systemName: "textformat.size.larger")
                    }
                    .labelsHidden()
                    Text("\(Int(ReaderStyle.clampedSize(size))) pt")
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                        .frame(width: 40, alignment: .trailing)
                }
            }

            Picker("Line width:", selection: $width) {
                ForEach(LineWidth.allCases) { option in
                    Text(option.displayName).tag(option)
                }
            }
            .pickerStyle(.segmented)

            LabeledContent("Preview:") {
                Text("The quick brown fox jumps over 1 234 lazy dogs. Il1 O0, « l'écart » se creuse.")
                    .font(font.swiftUIFont(size: ReaderStyle.clampedSize(size)))
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

private struct DataSettings: View {
    @AppStorage(DatabaseLocation.defaultsKey) private var databasePath = ""
    @State private var needsRelaunch = false

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
        guard path != databasePath else { return }
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
