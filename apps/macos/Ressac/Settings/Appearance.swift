/// The look of the reading window (spec §5, Switch): today's native window, or the Antiope
/// teletext set. Stored with `@AppStorage(Keys.appearance)` and applied live.
enum Appearance: String, CaseIterable, Identifiable, Sendable {
    case native, antiope

    var id: Self { self }

    var displayName: String {
        switch self {
        case .native: "Native"
        case .antiope: "Antiope"
        }
    }

    enum Keys {
        static let appearance = "appearance"
        /// The CRT screen checkbox (phase 13).
        static let crt = "antiope.crt"
    }

    /// Antiope only when Bedstead is available: the app never draws Antiope in a fallback font.
    static func effective(stored: Appearance, fontAvailable: Bool) -> Appearance {
        stored == .antiope && fontAvailable ? .antiope : .native
    }
}
