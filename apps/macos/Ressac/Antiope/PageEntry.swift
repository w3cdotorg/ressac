import Foundation

/// The three-digit page number being typed (spec §6, Page numbers): a digit starts it, the third
/// one completes it; ⌫ (CORRECTION) erases the last digit, Esc (ANNULATION) cancels it.
struct PageEntry: Equatable, Sendable {
    enum Outcome: Equatable, Sendable {
        case pending
        case complete(Int)
    }

    static let length = 3

    private(set) var digits = ""

    /// The digits typed so far, padded with `_` ("4__", "41_"), for the header; empty when idle.
    var display: String {
        isActive ? digits + String(repeating: "_", count: Self.length - digits.count) : ""
    }

    var isActive: Bool { !digits.isEmpty }

    /// Adds `digit` (0...9; any other value is ignored). The third digit completes the number and
    /// leaves the entry idle again.
    mutating func type(_ digit: Int) -> Outcome {
        guard (0...9).contains(digit) else { return .pending }
        digits.append(String(digit))
        guard digits.count == Self.length else { return .pending }
        let number = digits.reduce(0) { $0 * 10 + ($1.wholeNumberValue ?? 0) }
        digits = ""
        return .complete(number)
    }

    mutating func correct() {
        if !digits.isEmpty { digits.removeLast() }
    }

    mutating func cancel() {
        digits = ""
    }
}
