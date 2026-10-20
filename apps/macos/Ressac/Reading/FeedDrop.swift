import Foundation

/// Dragging a feed onto a folder of the sidebar (add-feed spec §7). The payload is plain text
/// carrying the feed id; anything else dropped on the sidebar is ignored.
enum FeedDrop: Equatable, Sendable {
    case ignore
    /// `nil` is the top level.
    case move(to: Int64?)

    private static let prefix = "ressac-feed:"

    static func payload(feedId: Int64) -> String { "\(prefix)\(feedId)" }

    /// The first dropped string that is exactly a feed payload (digits only).
    static func feedId(in items: [String]) -> Int64? {
        for item in items where item.hasPrefix(prefix) {
            let digits = item.dropFirst(prefix.count)
            if !digits.isEmpty, digits.allSatisfy({ $0.isASCII && $0.isNumber }), let id = Int64(digits) { return id }
        }
        return nil
    }

    /// A drop on the folder the feed already belongs to does nothing; so does a root feed on the "Feeds" header.
    static func decide(currentFolder: Int64?, dropFolder: Int64?) -> FeedDrop {
        currentFolder == dropFolder ? .ignore : .move(to: dropFolder)
    }
}
