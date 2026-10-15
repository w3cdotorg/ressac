import SwiftUI

/// The colour of each garden status, in both looks (spec §8). The one place that maps a
/// status to a colour: the sidebar's dot, the inspector's status line and the Antiope
/// garden pages all ask here. A nil status (a feed never diagnosed), "Unchecked" and a
/// status newer than this app share the neutral colour.
enum GardenStatusStyle {
    static func nativeColor(for status: String?) -> Color {
        switch status {
        case "Dead": .red
        case "Hijacked": .purple
        case "Moved": .blue
        case "Ailing": .orange
        case "Duplicate": .teal
        case "Fossil": .brown
        case "Dormant": .indigo
        case "Alive": .green
        default: .secondary
        }
    }

    /// Fossil is the only blue status: blue never colours text, so the Antiope garden draws
    /// its heading in reverse video, white on blue (phase 12).
    static func teletextColor(for status: String?) -> TeletextColor {
        switch status {
        case "Dead": .red
        case "Hijacked": .magenta
        case "Moved": .cyan
        case "Ailing": .yellow
        case "Duplicate": .white
        case "Fossil": .blue
        case "Dormant", "Alive": .green
        default: .white
        }
    }
}
