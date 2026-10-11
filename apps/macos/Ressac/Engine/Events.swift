import Foundation

/// The core's `service::Summary`.
struct Summary: Decodable, Sendable, Equatable {
    var total: Int
    /// Status name ("Dead", "Alive", …) → number of active feeds.
    var counts: [String: Int]
    var httpsRepairs: Int
    var redirectRepairs: Int
    var networkUnreachable: Bool

    private enum CodingKeys: String, CodingKey {
        case total, counts
        case httpsRepairs = "https_repairs"
        case redirectRepairs = "redirect_repairs"
        case networkUnreachable = "network_unreachable"
    }
}

/// One NDJSON line printed by `ressac --json` (the core's `service::Event`,
/// `#[serde(tag = "event", rename_all = "snake_case")]`).
enum EngineEvent: Decodable, Sendable, Equatable {
    case started(total: Int)
    case feedDone(feedId: Int64, title: String, error: String?)
    case networkUnreachable(failed: Int, attempted: Int)
    case probing(what: String, total: Int)
    case waybackDone(found: Int)
    case finished(summary: Summary?)
    case extracted(entryId: Int64, ok: Bool, error: String?)
    case decided(feedId: Int64, decision: String)
    case undone(feedId: Int64?)
    /// A JSON object this build does not know (for example the `import --json` report,
    /// which has no `event` key). Holds the raw line.
    case unknown(String)

    private enum CodingKeys: String, CodingKey {
        case event, total, title, error, failed, attempted, what, found, summary, ok, decision
        case feedId = "feed_id"
        case entryId = "entry_id"
    }

    private struct UnknownTag: Error {}

    init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        switch try c.decode(String.self, forKey: .event) {
        case "started":
            self = .started(total: try c.decode(Int.self, forKey: .total))
        case "feed_done":
            self = .feedDone(
                feedId: try c.decode(Int64.self, forKey: .feedId),
                title: try c.decode(String.self, forKey: .title),
                error: try c.decodeIfPresent(String.self, forKey: .error))
        case "network_unreachable":
            self = .networkUnreachable(
                failed: try c.decode(Int.self, forKey: .failed), attempted: try c.decode(Int.self, forKey: .attempted))
        case "probing":
            self = .probing(what: try c.decode(String.self, forKey: .what), total: try c.decode(Int.self, forKey: .total))
        case "wayback_done":
            self = .waybackDone(found: try c.decode(Int.self, forKey: .found))
        case "finished":
            self = .finished(summary: try c.decodeIfPresent(Summary.self, forKey: .summary))
        case "extracted":
            self = .extracted(
                entryId: try c.decode(Int64.self, forKey: .entryId),
                ok: try c.decode(Bool.self, forKey: .ok),
                error: try c.decodeIfPresent(String.self, forKey: .error))
        case "decided":
            self = .decided(
                feedId: try c.decode(Int64.self, forKey: .feedId), decision: try c.decode(String.self, forKey: .decision))
        case "undone":
            self = .undone(feedId: try c.decodeIfPresent(Int64.self, forKey: .feedId))
        default:
            throw UnknownTag()
        }
    }

    /// Decodes one stdout line. Nil for anything that is not a JSON object (logged and
    /// skipped by the caller, Review Focus #5); `.unknown(line)` for an object this build
    /// cannot read.
    static func decode(line: String) -> EngineEvent? {
        let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
        let data = Data(trimmed.utf8)
        guard trimmed.hasPrefix("{"),
            (try? JSONSerialization.jsonObject(with: data)) is [String: Any]
        else { return nil }
        return (try? JSONDecoder().decode(EngineEvent.self, from: data)) ?? .unknown(trimmed)
    }
}
