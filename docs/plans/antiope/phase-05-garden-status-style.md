# Phase 5: `GardenStatusStyle`, one colour per garden status

**Runs after:** phase 4.

Read the plan index first. Global Constraints apply. Spec §3 (In scope, last line), §5 (`TeletextPalette`: blue never colours text) and §8 (Garden pages, `GardenStatusStyle`).

**Prerequisites:** phase 3 (`TeletextColor`) and phase 6 (see phase 3's Execution order: with the v3 fixture from phase 1b, the Swift suite is only green once phase 6 has landed).

**Files (4):**
- Create: `apps/macos/Ressac/Garden/GardenStatusStyle.swift`
- Modify: `apps/macos/Ressac/Reading/MainView.swift` (`SidebarRow`, lines 110–147: the dot's colour on line 122 and `static func color(for:)` on lines 138–146)
- Modify: `apps/macos/Ressac/Garden/GardenInspector.swift` (line 50, the status line; lines 163–174, `private func statusColor(_:)`)
- Modify: `apps/macos/RessacTests/GardenTests.swift` (append one suite)

**Step 0:** nothing to clean. `MainView.swift` has 247 lines and `GardenInspector.swift` 226, both under 300; this phase removes code from them and restructures nothing.

**Test command:**

```sh
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)/apps/macos" && xcodegen generate --quiet && \
  xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test \
    -only-testing:RessacTests/GardenStatusStyleTests \
  2>&1 | grep -E '^/.+: error:|✘|Test run|\*\* TEST'
```

**The two copies disagree, so "moved verbatim" needs one decision.** Today:

| Status | `SidebarRow.color(for:)` (sidebar dot, sick statuses only) | `GardenInspector.statusColor(_:)` (status line) |
|---|---|---|
| Dead | red | red |
| Hijacked | **purple** | **red** |
| Moved | blue | blue |
| Ailing | orange (the `default` branch) | orange |
| Duplicate | **teal** | **purple** |
| Fossil | (never shown) | brown |
| Dormant | (never shown) | indigo |
| Alive | (never shown) | green |
| other, `Unchecked` | (never shown) | secondary |

`nativeColor(for:)` keeps every value both copies agree on, the sidebar's values where they differ (Hijacked purple, Duplicate teal; Ailing orange, the sidebar's default for its other sick status) and the inspector's values for the statuses only it shows (Fossil brown, Dormant indigo, Alive green, anything else `.secondary`). This is Clément's decision of 2026-10-14 (spec §8). Reasons: the inspector's red Hijacked cannot be told from Dead, while the sidebar's five sick colours are all distinct; and purple and teal follow the teletext hues (magenta, and a neutral) better. **Visible change:** the inspector's status line turns purple for Hijacked (was red) and teal for Duplicate (was purple); the sidebar does not change.

### Task 5.1: One style for the garden statuses, used by the sidebar and the inspector

**Interfaces:**
- Consumes: `TeletextColor` (phase 3); `GardenModel.urgencyOrder` (`["Duplicate", "Hijacked", "Moved", "Dead", "Ailing", "Fossil", "Dormant", "Alive", "Unchecked"]`); `FeedItem.isSick` statuses (`Ailing`, `Dead`, `Moved`, `Hijacked`, `Duplicate`).
- Produces (`Garden/GardenStatusStyle.swift`), exactly as the plan index:
  - `enum GardenStatusStyle`;
  - `static func nativeColor(for status: String?) -> Color`: Dead red, Hijacked purple, Moved blue, Ailing orange, Duplicate teal, Fossil brown, Dormant indigo, Alive green, anything else (nil, `Unchecked`, a newer status) `.secondary`;
  - `static func teletextColor(for status: String?) -> TeletextColor`: Dead red, Hijacked magenta, Moved cyan, Ailing yellow, Duplicate white, Fossil blue, Dormant green, Alive green, anything else white. Phase 12 draws the Fossil heading in reverse video (white on blue), since blue never colours text.
- Removes: `SidebarRow.color(for:)` (`MainView.swift`) and `GardenInspector.statusColor(_:)`. Both callers pass a non-optional `String`, which converts to `String?` implicitly.

- [ ] **Step 1: Write the failing test.** Append to `apps/macos/RessacTests/GardenTests.swift` (the file already imports `SwiftUI`, `Testing` and `@testable import Ressac`):

```swift
@Suite struct GardenStatusStyleTests {
    @Test func everyStatusHasOneNativeAndOneTeletextColour() {
        let expected: [(String?, Color, TeletextColor)] = [
            ("Dead", .red, .red),
            ("Hijacked", .purple, .magenta),
            ("Moved", .blue, .cyan),
            ("Ailing", .orange, .yellow),
            ("Duplicate", .teal, .white),
            ("Fossil", .brown, .blue),
            ("Dormant", .indigo, .green),
            ("Alive", .green, .green),
            ("Unchecked", .secondary, .white),
            ("Haunted", .secondary, .white),  // a status newer than this app
            (nil, .secondary, .white),
        ]
        for (status, native, teletext) in expected {
            #expect(GardenStatusStyle.nativeColor(for: status) == native, "\(status ?? "nil")")
            #expect(GardenStatusStyle.teletextColor(for: status) == teletext, "\(status ?? "nil")")
        }
    }

    /// The sidebar dot tells the sick statuses apart, Dead and Hijacked included.
    @Test func sickStatusesNeverShareANativeColour() {
        let sick = ["Ailing", "Dead", "Moved", "Hijacked", "Duplicate"]
        #expect(Set(sick.map { GardenStatusStyle.nativeColor(for: $0) }).count == sick.count)
    }

    /// Blue never colours text (spec §5): only Fossil is blue, and its heading is drawn reversed.
    @Test func onlyFossilIsBlueAndNoStatusIsBlack() {
        let statuses = GardenModel.urgencyOrder
        #expect(statuses.filter { GardenStatusStyle.teletextColor(for: $0) == .blue } == ["Fossil"])
        #expect(!statuses.contains { GardenStatusStyle.teletextColor(for: $0) == .black })
    }
}
```

- [ ] **Step 2: Run red** (test command above). Expected: `error: cannot find 'GardenStatusStyle' in scope`, then `** TEST FAILED **`.

- [ ] **Step 3: Implement.** Create `apps/macos/Ressac/Garden/GardenStatusStyle.swift`:

```swift
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
```

- [ ] **Step 4: Run green**, same command. Expected: `✔ Test run with 3 tests in 1 suite passed`, `** TEST SUCCEEDED **`.

- [ ] **Step 5: Use it in the sidebar.** Re-read `apps/macos/Ressac/Reading/MainView.swift` first (Edit Safety). In `SidebarRow.body`, replace:

```swift
                    Circle().fill(Self.color(for: status)).frame(width: 7, height: 7).help(status)
```

with:

```swift
                    Circle().fill(GardenStatusStyle.nativeColor(for: status)).frame(width: 7, height: 7).help(status)
```

Then delete the whole method at the end of `SidebarRow`, with the blank line before it, so that the struct ends right after `body`:

```swift

    static func color(for status: String) -> Color {
        switch status {
        case "Dead": .red
        case "Hijacked": .purple
        case "Moved": .blue
        case "Duplicate": .teal
        default: .orange
        }
    }
```

- [ ] **Step 6: Use it in the inspector.** Re-read `apps/macos/Ressac/Garden/GardenInspector.swift` first. In `details(_:)`, replace:

```swift
                    .foregroundStyle(statusColor(status))
```

with:

```swift
                    .foregroundStyle(GardenStatusStyle.nativeColor(for: status))
```

Then delete the method and the blank line after it, so that `labeled(_:_:)` is followed directly by `observe(_:into:)`:

```swift
    private func statusColor(_ status: String) -> Color {
        switch status {
        case "Dead", "Hijacked": .red
        case "Ailing": .orange
        case "Moved": .blue
        case "Duplicate": .purple
        case "Fossil": .brown
        case "Dormant": .indigo
        case "Alive": .green
        default: .secondary
        }
    }

```

- [ ] **Step 7: Check that no other copy or caller is left.** Each search on its own (project rule 10):

```bash
cd "$(git rev-parse --show-toplevel)/apps/macos"
grep -rn "statusColor" Ressac RessacTests
grep -rn "color(for:" Ressac RessacTests
grep -rn "SidebarRow\.color\|Self\.color(" Ressac RessacTests
grep -rn '"color(for\|"statusColor' Ressac RessacTests
grep -rnE '\.(purple|teal|brown|indigo)\b' Ressac
```

Expected: the first four print nothing; the last prints only lines of `Ressac/Garden/GardenStatusStyle.swift`. No test, mock, re-export or dynamic lookup referred to the two removed methods (checked on 2026-10-14: `grep` finds them only at their definitions and their single call sites).

- [ ] **Step 8: Run the garden and reading suites.** `-only-testing:RessacTests/GardenStatusStyleTests -only-testing:RessacTests/GardenModelTests -only-testing:RessacTests/AppStateTests`. Expected: all pass, `** TEST SUCCEEDED **`, and no warning (an unused `Color` import or a leftover `Self.color` would fail the build under warnings as errors).

- [ ] **Step 9: Commit.**

```bash
git add apps/macos/Ressac/Garden/GardenStatusStyle.swift apps/macos/Ressac/Reading/MainView.swift \
        apps/macos/Ressac/Garden/GardenInspector.swift apps/macos/RessacTests/GardenTests.swift
git commit -F - <<'EOF'
macOS app: one GardenStatusStyle for the sidebar, the inspector and Antiope

The two colour copies disagreed on Hijacked and Duplicate; the sidebar's
values win, so the inspector's status line changes for those two.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Verification gate

```sh
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/macos && xcodegen generate && \
  xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test 2>&1 | tee "${TMPDIR:-/tmp}/ressac-xcodebuild.log" | tail -3
grep -E '^/.+/apps/macos/.+: (warning|error):' "${TMPDIR:-/tmp}/ressac-xcodebuild.log" || echo "no warnings in our sources"
```

Expected: Rust checks unchanged, `** TEST SUCCEEDED **` (3 more tests than after phase 4), `no warnings in our sources`.

**Manual look (no keystrokes sent to any app):** launch a Debug build on a throwaway copy of the fixture, as mac-app phase 11 does (`RESSAC_DB` and `HOME` in a temp dir), set one feed's health to Hijacked with `sqlite3 "$SMOKE/ressac.sqlite" "INSERT OR REPLACE INTO feed_health (feed_id, status, reason, computed_at, needs_triage) VALUES (2, 'Hijacked', 'Test.', '2026-10-14T08:00:00.000Z', 1)"`, then check that the sidebar dot of Orange Drenka and the inspector's status line (Garden ▸ Needs Triage) are both purple. Quit, `rm -rf "$SMOKE"`. Push:

```bash
git -c credential.helper= -c credential.helper='!gh auth git-credential' push
```

### Notes for the executor

- **Verified ahead of time.** `GardenStatusStyle.swift`, both edits and the new suite were built and run in a scratch copy of the repository (Xcode 27, Swift 6.4, warnings as errors), with the phase 3 module present.
- **`Color` is `Hashable`**, so the table compares colours with `==` and the distinctness test builds a `Set`. `.secondary` is a `Color` static (`Color.secondary`), so it fits the same table.
- **`GardenInspector` passes `GardenModel.status(of:)`**, which is `"Unchecked"` for a feed never diagnosed: it keeps its neutral `.secondary`, as before.
- **Phase 12** draws the Antiope garden headings with `teletextColor(for:)`; it owns the reverse-video rule for Fossil. Nothing else in the app should map a status to a colour from now on.
