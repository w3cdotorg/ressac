import SwiftUI

@main
struct RessacApp: App {
    @State private var launcher = Launcher()

    var body: some Scene {
        WindowGroup("Ressac") {
            LaunchView(launcher: launcher)
                .frame(minWidth: 900, minHeight: 500)
                .task {
                    // The unit-test host must not open the real database.
                    guard !Launcher.isRunningTests else { return }
                    await launcher.launch(database: DatabaseLocation.current(), executable: RessacCLI.bundledExecutable())
                }
        }
        .commands { RessacCommands(state: launcher.state) }

        Settings {
            SettingsView()
        }
    }
}

/// What the window shows for each launch phase (spec §8, Schema version).
struct LaunchView: View {
    let launcher: Launcher
    @AppStorage(Appearance.Keys.appearance) private var appearance = Appearance.native

    var body: some View {
        switch launcher.phase {
        case .starting:
            ProgressView()
        case .migrating:
            ProgressView("Updating the database…")
        case .tooNew:
            VStack(spacing: 12) {
                Text("This database was created by a newer Ressac").font(.title3)
                Button("Use Default Database") { Task { await launcher.useDefaultDatabase() } }
            }
        case let .failed(message):
            VStack(spacing: 12) {
                Text(message).multilineTextAlignment(.center)
                HStack {
                    Button("Use Default Database") { Task { await launcher.useDefaultDatabase() } }
                    Button("Quit Ressac") { NSApplication.shared.terminate(nil) }
                }
            }
            .padding(40)
        case let .ready(state):
            // Both looks read the same `state`, so a switch keeps the selection, the selected
            // entry, the search text and a running refresh (spec §5, Switch). The library chrome
            // wraps the switch: an OPML import in progress survives it too.
            Group {
                switch Appearance.effective(stored: appearance, fontAvailable: TeletextFont.isAvailable) {
                case .native: MainView(state: state)
                case .antiope: AntiopeRootView(state: state)
                }
            }
            .libraryChrome(state)
            .environment(state)
        }
    }
}
