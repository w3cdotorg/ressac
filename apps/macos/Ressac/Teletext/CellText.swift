/// A run of cells: text Bedstead draws, or `?` standing for graphemes it lacks, which the
/// views draw in magenta (spec §5, `CellText`).
struct CellRun: Equatable, Sendable {
    var text: String
    var isReplacement: Bool
}

struct FittedCells: Equatable, Sendable {
    /// Exactly `cells` characters in total, one per cell, replacements as "?".
    var runs: [CellRun]
    /// The original string, untouched: VoiceOver reads this, never the cells.
    var accessibilityText: String
}

/// Fits a string into an exact number of character cells, so that no row can push a column
/// separator (spec §2, Layout).
enum CellText {
    /// One grapheme per cell. Truncation cuts between graphemes, without an ellipsis (teletext
    /// had none); short text is padded with spaces; adjacent runs of the same kind are merged.
    static func fit(_ text: String, cells: Int, supports: (Character) -> Bool = TeletextFont.supports) -> FittedCells {
        let width = max(cells, 0)
        var runs: [CellRun] = []
        var used = 0
        func add(_ piece: String, replacement: Bool) {
            if runs.last?.isReplacement == replacement {
                runs[runs.count - 1].text += piece
            } else {
                runs.append(CellRun(text: piece, isReplacement: replacement))
            }
        }
        for grapheme in text {
            guard used < width else { break }
            guard let cell = cell(for: grapheme, supports: supports) else { continue }
            add(String(cell.character), replacement: cell.isReplacement)
            used += 1
        }
        if used < width {
            add(String(repeating: " ", count: width - used), replacement: false)
        }
        return FittedCells(runs: runs, accessibilityText: text)
    }

    /// What one grapheme draws: a space for any whitespace (tabs and line breaks included),
    /// nothing for invisible format or control characters (zero-width space, BOM, soft hyphen),
    /// the composed form (NFC) when Bedstead has it, else a replacement "?".
    private static func cell(
        for grapheme: Character, supports: (Character) -> Bool
    ) -> (character: Character, isReplacement: Bool)? {
        if grapheme.isWhitespace { return (" ", false) }
        if grapheme.unicodeScalars.allSatisfy({ invisible.contains($0.properties.generalCategory) }) { return nil }
        let composed = String(grapheme).precomposedStringWithCanonicalMapping
        guard composed.count == 1, let character = composed.first, supports(character) else { return ("?", true) }
        return (character, false)
    }

    private static let invisible: Set<Unicode.GeneralCategory> = [.format, .control]
}
