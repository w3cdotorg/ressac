# Phase 8: Antiope minors, kept date formatters and buttons out of the keyboard loop

**Runs after:** phase 7, the last of phases 1 to 7, applied in order. One of the five files below was changed by an earlier phase: phase 4b rewrites `GardenPage.feedLine` in `Antiope/GardenPage.swift` (338 → 346 lines), so `logDate` moves from lines 287-293 to 295-301; the line numbers below are those after phase 7, and the anchors are the quoted texts. Phase 4b also inserts a test in `AntiopeGardenTests.swift`, which this phase only runs. No other phase touches `AntiopeRootView.swift`, `SpecialPages.swift`, `GardenPageViews.swift` or `AntiopeFrameTests.swift`.

**Goal.** Close two of the "Mineurs des relectures" of `TODO.md` (« Reporté ▸ Antiope »):
- the page header and the garden's fetch log build a new `DateFormatter` on every call; the header calls it twice a second at rest and twenty times a second while the page counter rolls (`TimelineView(.periodic(from: .now, by: rolling ? 0.1 : 1))`, `AntiopeRootView.swift:134`). A shared cache keeps one formatter per pattern and time zone, with the exact same output;
- P100's page rows and the banner's LOG and X keys are still focusable under Full Keyboard Access, where Space would press the focused button instead of reaching SUITE. They get `.focusable(false)`, as the index rows, the entry rows, the column title and the Minitel keys already have (`PageColumns.swift:21`, `:90`, `:221`, `MinitelBar.swift:68`).

The third minor of that line, `NSWorkspace` called directly by the native banner and `GardenInspector`, is done in phase 7 (Task 7.4), which had room for its two files. The fourth, splitting `main.rs`, is phase 2.

**Files (5):**
- Modify: `apps/macos/Ressac/Antiope/AntiopeRootView.swift` (`PageHeader`, and the new `TeletextDateFormat` after it)
- Modify: `apps/macos/Ressac/Antiope/GardenPage.swift` (`logDate`)
- Modify: `apps/macos/Ressac/Antiope/SpecialPages.swift` (`SommairePage.pageButton`)
- Modify: `apps/macos/Ressac/Antiope/GardenPageViews.swift` (`AntiopeBannerRow`)
- Modify: `apps/macos/RessacTests/AntiopeFrameTests.swift` (a new suite at the end)

**Step 0.** `GardenPage.swift` has 346 lines (after phase 4b) and `AntiopeFrameTests.swift` 442, but this phase only replaces one function body in the first and appends a suite to the second: no restructuring, so no Step 0 commit. The audit finds nothing to remove anyway:

```bash
cd /Users/willow/Sites/ressac/apps/macos
wc -l Ressac/Antiope/GardenPage.swift Ressac/Antiope/AntiopeRootView.swift RessacTests/AntiopeFrameTests.swift
grep -nE 'print\(|debugPrint|NSLog|dump\(|TODO|FIXME' Ressac/Antiope/GardenPage.swift Ressac/Antiope/AntiopeRootView.swift
```

Expected: `346`, `283` and `442` lines; the `grep` prints nothing.

**Builds on (exact, current code):**
- `PageHeader` (`AntiopeRootView.swift:115`) is a SwiftUI `View`, so its static functions are main-actor isolated; `PageHeader.line(label:date:width:rightMargin:timeZone:) -> CellLine` formats `"EEE dd MMM"` and `"HH:mm:ss"` through `private static func format(_:_:timeZone:)` (lines 202 to 209: `en_US_POSIX`, Gregorian calendar, the given time zone).
- `GardenPage` (`GardenPage.swift:7`) is a plain enum, not isolated; `static func logDate(_ date: Date, timeZone: TimeZone) -> String` (lines 295 to 301) formats `"dd MMM yy HH:mm"` in `en_US_POSIX` and uppercases it. It sets no calendar; `en_US_POSIX`'s calendar is Gregorian, so the shared formatter, which sets it, gives the same text.
- Existing tests that pin the output: `PageHeaderTests` (`AntiopeFrameTests.swift:311`, e.g. `["P412 ", "RESSAC          ", "WED 14 OCT ", "09:47:12"]` in Paris time) and `GardenInspectorPageTests.logDatesAreTeletextCapitals` (`AntiopeGardenTests.swift:313`, `"10 OCT 26 07:00"`).

**Why a locked `DateFormatter` cache and not `Date.VerbatimFormatStyle`.** The format style is `Sendable` and cached by Foundation, but it does not round fractional seconds as `DateFormatter` does: on 20 000 random dates in six time zones it printed a different second about once in 2 000 dates (`"17:33:34"` against `"17:33:35"`), and the header formats `TimelineView` dates, which carry fractions. The cache keeps `DateFormatter`, so the text cannot change. A `DateFormatter` is not `Sendable`, so the cache lives in a `Mutex` (module `Synchronization`, macOS 15 and later; the app targets macOS 27) and formats inside the lock: only the `String` leaves it. Measured in a release build: about 150 µs to build and use a formatter, under 1 µs to use a kept one.

**How to run one test.**

```bash
cd /Users/willow/Sites/ressac/apps/macos
xcodegen generate
xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test \
  -only-testing:'RessacTests/TeletextDateFormatTests'
```

---

### Task 8.1: one `DateFormatter` per pattern and time zone

**Files:**
- Modify: `apps/macos/Ressac/Antiope/AntiopeRootView.swift:1-2`, `:173-174`, `:198-210`
- Modify: `apps/macos/Ressac/Antiope/GardenPage.swift:294-301`
- Test: `apps/macos/RessacTests/AntiopeFrameTests.swift` (after line 442, the end of the file)

**Interfaces:**
- Produces: `enum TeletextDateFormat` (not isolated) with `static func string(from date: Date, pattern: String, timeZone: TimeZone) -> String` and `static func isCached(_ pattern: String, timeZone: TimeZone) -> Bool` (for the tests).

- [ ] **Step 1: Write the failing tests.** Append to `AntiopeFrameTests.swift`, after its last line (`}` closing `AntiopeRootTests`, line 442):

```swift

/// The header formats the date and the time every second: its `DateFormatter`s are built once
/// per pattern and time zone, and give what a fresh one gives.
@MainActor @Suite struct TeletextDateFormatTests {
    @Test func formatsAsAFreshDateFormatter() {
        let zones = ["Europe/Paris", "GMT", "Asia/Kolkata", "Pacific/Chatham"].compactMap(TimeZone.init(identifier:))
        #expect(zones.count == 4)
        for zone in zones {
            for pattern in ["EEE dd MMM", "HH:mm:ss", "dd MMM yy HH:mm"] {
                let fresh = DateFormatter()
                fresh.locale = Locale(identifier: "en_US_POSIX")
                fresh.calendar = Calendar(identifier: .gregorian)
                fresh.timeZone = zone
                fresh.dateFormat = pattern
                // A year and a half of dates, a second short of a day apart, fractions included.
                for step in 0..<550 {
                    let date = Date(timeIntervalSince1970: 1_791_615_600.25 + Double(step) * 86_399.5)
                    #expect(TeletextDateFormat.string(from: date, pattern: pattern, timeZone: zone)
                            == fresh.string(from: date), "\(pattern) \(zone.identifier) \(date)")
                }
            }
        }
    }

    @Test func theHeaderAndTheFetchLogKeepTheirFormatters() throws {
        // Offsets no other test uses: only these two calls can have filled these keys.
        let header = try #require(TimeZone(secondsFromGMT: 3 * 3600 + 17 * 60))
        let log = try #require(TimeZone(secondsFromGMT: -(5 * 3600 + 43 * 60)))
        _ = PageHeader.line(label: .page(412), date: .now, width: 40, timeZone: header)
        _ = GardenPage.logDate(.now, timeZone: log)
        #expect(TeletextDateFormat.isCached("EEE dd MMM", timeZone: header))
        #expect(TeletextDateFormat.isCached("HH:mm:ss", timeZone: header))
        #expect(TeletextDateFormat.isCached("dd MMM yy HH:mm", timeZone: log))
    }
}
```

- [ ] **Step 2: Run them to see them fail.**

Run: the "How to run one test" command above.
Expected: the test target does not compile, `error: cannot find 'TeletextDateFormat' in scope`; `** TEST FAILED **`.

- [ ] **Step 3: Implement.** In `AntiopeRootView.swift`, replace lines 1 and 2:

```swift
import AppKit
import SwiftUI
```

with:

```swift
import AppKit
import Synchronization
import SwiftUI
```

Replace lines 173 and 174 (in `PageHeader.line`):

```swift
        let day = format(date, "EEE dd MMM", timeZone: timeZone).uppercased() + " "
        let time = format(date, "HH:mm:ss", timeZone: timeZone)
```

with:

```swift
        let day = TeletextDateFormat.string(from: date, pattern: "EEE dd MMM", timeZone: timeZone).uppercased() + " "
        let time = TeletextDateFormat.string(from: date, pattern: "HH:mm:ss", timeZone: timeZone)
```

Replace lines 198 to 210 (the end of `PageHeader.line`, `format`, and the closing brace of `PageHeader`):

```swift
        if margin > 0 { spans.append(CellSpan("", cells: margin, foreground: .white)) }
        return CellLine(spans)
    }

    private static func format(_ date: Date, _ pattern: String, timeZone: TimeZone) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.timeZone = timeZone
        formatter.dateFormat = pattern
        return formatter.string(from: date)
    }
}
```

with:

```swift
        if margin > 0 { spans.append(CellSpan("", cells: margin, foreground: .white)) }
        return CellLine(spans)
    }
}

/// Fixed-pattern dates for the teletext pages, in English whatever the system language. One
/// `DateFormatter` per pattern and time zone, kept: the header formats the date and the time every
/// second (every tenth of a second while the counter rolls), and building a `DateFormatter` costs
/// about a hundred times more than using one.
enum TeletextDateFormat {
    private struct Key: Hashable {
        var pattern: String
        var timeZone: TimeZone
    }

    /// A handful of keys in practice: three patterns, times the zones the app has run in.
    private static let formatters = Mutex<[Key: DateFormatter]>([:])

    static func string(from date: Date, pattern: String, timeZone: TimeZone) -> String {
        formatters.withLock { cache in
            let key = Key(pattern: pattern, timeZone: timeZone)
            if let formatter = cache[key] { return formatter.string(from: date) }
            let formatter = DateFormatter()
            formatter.locale = Locale(identifier: "en_US_POSIX")
            formatter.calendar = Calendar(identifier: .gregorian)
            formatter.timeZone = timeZone
            formatter.dateFormat = pattern
            cache[key] = formatter
            return formatter.string(from: date)
        }
    }

    /// Whether `pattern` in `timeZone` already has its formatter (for the tests).
    static func isCached(_ pattern: String, timeZone: TimeZone) -> Bool {
        formatters.withLock { $0[Key(pattern: pattern, timeZone: timeZone)] != nil }
    }
}
```

In `GardenPage.swift`, replace lines 295 to 301:

```swift
    static func logDate(_ date: Date, timeZone: TimeZone) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = timeZone
        formatter.dateFormat = "dd MMM yy HH:mm"
        return formatter.string(from: date).uppercased()
    }
```

with:

```swift
    static func logDate(_ date: Date, timeZone: TimeZone) -> String {
        TeletextDateFormat.string(from: date, pattern: "dd MMM yy HH:mm", timeZone: timeZone).uppercased()
    }
```

Its doc comment (line 294, `/// "10 OCT 26 07:00": fetch log dates, English month in capitals, in `timeZone`.`) stays.

- [ ] **Step 4: Run the new suite and the suites that pin the old output.**

Run: `-only-testing:'RessacTests/TeletextDateFormatTests' -only-testing:'RessacTests/PageHeaderTests' -only-testing:'RessacTests/GardenInspectorPageTests'` (`AntiopeGardenTests.swift` has no `AntiopeGardenTests` suite; `logDatesAreTeletextCapitals` is in `GardenInspectorPageTests`).
Expected: `✔ Test run with 13 tests in 3 suites passed`, `** TEST SUCCEEDED **`, no Swift warning.

- [ ] **Step 5: Commit.**

```bash
cd /Users/willow/Sites/ressac
git add apps/macos/Ressac/Antiope/AntiopeRootView.swift apps/macos/Ressac/Antiope/GardenPage.swift apps/macos/RessacTests/AntiopeFrameTests.swift
git commit -m "$(cat <<'EOF'
Antiope: keep one DateFormatter per pattern and time zone for the header and the fetch log

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 8.2: P100 rows and the banner keys leave the Full Keyboard Access loop

**Files:**
- Modify: `apps/macos/Ressac/Antiope/SpecialPages.swift:70-75`
- Modify: `apps/macos/Ressac/Antiope/GardenPageViews.swift:139-151`

**Interfaces:** none change.

There is no in-process test for this task: whether a SwiftUI button joins the key-view loop is not observable from the test target without UI automation, and simulated keystrokes are ruled out. The commit that gave the other Antiope buttons `.focusable(false)` (`8a5bf4a`) had no test either. The verification is the `grep` in Step 3, the build, and Clément's manual check at the gate. The mouse keeps working: `.focusable(false)` only takes the button out of the keyboard loop; VoiceOver still reads and presses it.

- [ ] **Step 1: P100's page rows.** In `SpecialPages.swift`, replace lines 70 to 75:

```swift
    private func pageButton(_ row: PageDirectory.Row, width: Int, in directory: PageDirectory) -> some View {
        let line = Self.line(for: row, width: width, in: directory)
        return Button { antiope.jump(to: row.number) } label: { CellLineView(line: line) }
            .buttonStyle(.plain)
            .accessibilityLabel(line.accessibilityText)
    }
```

with:

```swift
    private func pageButton(_ row: PageDirectory.Row, width: Int, in directory: PageDirectory) -> some View {
        let line = Self.line(for: row, width: width, in: directory)
        return Button { antiope.jump(to: row.number) } label: { CellLineView(line: line) }
            .buttonStyle(.plain)
            .focusable(false)  // Full Keyboard Access would give Space to the button, not to SUITE
            .accessibilityLabel(line.accessibilityText)
    }
```

(The comment is the one on the index rows, `PageColumns.swift:21`.)

- [ ] **Step 2: The banner's LOG and X keys.** In `GardenPageViews.swift` (`AntiopeBannerRow`, shown on every Antiope page), replace lines 139 to 151:

```swift
                if let log = row.log {
                    Button {
                        state.openURL(DatabaseLocation.logsURL)
                    } label: {
                        CellLineView(line: log)
                    }
                    .buttonStyle(.plain)
                }
                Button(action: dismiss) {
                    CellLineView(line: row.dismiss)
                }
                .buttonStyle(.plain)
            }
```

with:

```swift
                if let log = row.log {
                    Button {
                        state.openURL(DatabaseLocation.logsURL)
                    } label: {
                        CellLineView(line: log)
                    }
                    .buttonStyle(.plain)
                    .focusable(false)
                }
                Button(action: dismiss) {
                    CellLineView(line: row.dismiss)
                }
                .buttonStyle(.plain)
                .focusable(false)
            }
```

- [ ] **Step 3: Check every plain Antiope button, and build.**

```bash
cd /Users/willow/Sites/ressac/apps/macos/Ressac/Antiope
grep -n -A6 'Button' SpecialPages.swift GardenPageViews.swift | grep -E 'Button|focusable'
cd /Users/willow/Sites/ressac/apps/macos
xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' build
```

Expected: in `SpecialPages.swift`, the three `pageButton` lines (57, 60, 70), then the `Button` at line 72 followed by `.focusable(false)` at line 74; in `GardenPageViews.swift`, the two `Button`s of `AntiopeBannerRow` (lines 140 and 148) followed by `.focusable(false)` at lines 146 and 152, and three lines left alone on purpose: the two buttons of the safe-repairs confirmation alert (lines 48 and 49, system alert buttons, which must stay focusable) and the hidden Undo Garden Decision button (line 67, see Notes). `** BUILD SUCCEEDED **`, no warning.

- [ ] **Step 4: Commit.**

```bash
cd /Users/willow/Sites/ressac
git add apps/macos/Ressac/Antiope/SpecialPages.swift apps/macos/Ressac/Antiope/GardenPageViews.swift
git commit -m "$(cat <<'EOF'
Antiope: P100 rows and the banner keys are not focusable under Full Keyboard Access

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Verification gate (phase 8)

- [ ] **Rust:**

```bash
cd /Users/willow/Sites/ressac
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: no output from `fmt`, no clippy warning, all tests pass, with the counts of phase 7 (`ressac` 35, `cli` 19, core lib 260 + 1 ignored, `article_proxy` 1, `contract` 9, `end_to_end` 4); this phase changes no Rust.

- [ ] **Swift:**

```bash
cd /Users/willow/Sites/ressac/apps/macos
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
log=$(mktemp)
xcodegen generate
xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test 2>&1 | tee "$log" | tail -3
grep -E '\.swift:[0-9]+:[0-9]+: warning:' "$log"
```

Expected: `✔ Test run with 347 tests in 84 suites passed` (the 345 of phase 7 plus the two of Task 8.1), `** TEST SUCCEEDED **`; the `grep` prints nothing (zero Swift warnings). Checked on 15/10/2026 after phases 1 to 7 applied in order; the CI count step of Task 7.5, dry-run on that result bundle, printed `RessacTests declares 347 tests; 347 ran (0 failed, 0 skipped).`

- [ ] **Manual check by Clément (no simulated keystrokes),** in the Antiope look, with System Settings > Accessibility > Keyboard > Full Keyboard Access on:
  1. P100: Tab never lands on a page row; Space turns the page (SUITE). A click on a row still jumps to its page.
  2. Make a refresh fail (for instance with the network off) so that the red banner shows: Tab never lands on LOG or X; clicking LOG opens `~/Library/Logs/Ressac`, clicking X dismisses the banner.
  3. The header clock still ticks every second and the counter still rolls during a refresh, in the same text as before (`WED 14 OCT 09:47:12`).

---

### Notes for the executor

- **Hidden Undo button, left alone.** `GardenPageViews.swift:67` is a zero-size, transparent `Button("Undo Garden Decision")` that carries the ⌘Z shortcut. If Clément's check 2 shows Tab landing on an invisible control on the garden pages, that button is the likely cause; adding `.focusable(false)` to it should keep ⌘Z (a key equivalent does not need focus), but that was not verified here, so it is not part of this phase.
- **Out of scope:** the `PageDirectory` cache that `notes.md:149` lists with the formatter one (it is not in the TODO line), the large-folder scroll hitches (postponed by Clément on 15/10/2026), and the `main.rs` split (phase 2).
- **The formatter cache grows only with new (pattern, time zone) pairs:** three patterns today, times the time zones the app sees during one run. No eviction is needed.
- **TODO.md:** phase 9 ticks the whole "Mineurs des relectures" line: formatters and focus here, `NSWorkspace` in phase 7, the `main.rs` split in phase 2.
