# Phase 10: The reader in Antiope (10a: sizes and widths; 10b: stylesheet, theme and sub-page line)

**Runs after:** phase 9 (10a, then 10b).

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

Read the plan index first (`docs/plans/2026-10-14-antiope.md`); its Global Constraints apply to every task. Spec: §5 (Settings: text size steps, 40 columns, the CRT box), §6 (Sub-pages: "412 3/20"), §7 (The reader in Antiope: Stylesheet), §9 (Colour bleed, reader side), §13 (the `scaleY(2)` risk).

**Goal:** Articles in Antiope are drawn as teletext pages: Bedstead at 15, 20, 25 or 30 pt, the eight colours on black, a double-height yellow title, the sub-page line "412 3/20", an optional 40-column width, and the CRT colour bleed when the screen box is checked.

**Architecture:** The reader stays one `WKWebView` page built by `ArticleTemplate`. A `ReaderTheme` (appearance + CRT) travels with `ArticlePage`; the template marks `:root` with `data-theme="antiope"` (and `data-crt`), embeds Bedstead as a `data:` font like the bundled reader fonts, and `reader.css` gains one Antiope section that redefines every colour variable and puts every vertical distance on a 1.2 em row. Each look keeps its own stored text size (decision of 2026-10-14): Native `reader.size` (12...28, default 17), Antiope `reader.size.antiope` (`ReaderStyle.Keys.antiopeSize`, phase 3; 15, 20, 25 or 30, default 20). Antiope snaps its size to Bedstead's steps when drawing; ⌘+, ⌘− and ⌘0 and the Settings slider act on the size of the look shown and never touch the other. The sub-page line changes in place through app JavaScript, so a refresh that grows the list never reloads the page.

**Tech Stack:** Swift 6.4 (Swift 6 mode, complete strict concurrency, warnings as errors), SwiftUI, WebKit, Swift Testing, CSS. No new dependency.

**Spec:** `docs/specs/2026-10-14-antiope-design.md`.

## This phase is split in two

The work touches seven files (`ArticleTemplate.swift`, `ReaderStyle.swift`, `Commands.swift`, `SettingsView.swift`, `reader.css`, `ReaderView.swift`, `ReaderTests.swift`), over the five-file limit, so it runs as two phases, each with its own verification gate and Clément's approval before the next. The index table gets rows **10a** and **10b** in place of row 10 (a plan change, reported with this file).

| Phase | Files (5 at most, tests included) | Deliverable |
|---|---|---|
| 10a | `Reading/ArticleTemplate.swift` (Step 0 only), `Settings/ReaderStyle.swift`, `App/Commands.swift`, `Settings/SettingsView.swift`, `RessacTests/ReaderTests.swift` | Step 0 cleanup; `LineWidth.forty`, `ReaderTheme`, one stored size and size steps per look; ⌘+ / ⌘− / ⌘0 and the Settings slider act on the look shown |
| 10b | `Reading/ArticleTemplate.swift`, `Resources/reader.css`, `Reading/ReaderView.swift`, `RessacTests/ReaderTests.swift` | `SubpageLabel`; the Antiope page (`data-theme`, `data-crt`, Bedstead, double height, sub-page line); the Antiope and CRT stylesheet; the reader view passing the theme and updating the sub-page line in place |

## Interfaces

**Consumes (pinned in the index, from earlier phases):**
- Phase 3: `ReaderStyle.Keys.antiopeSize` (`"reader.size.antiope"`, Antiope's own stored size); `TeletextFont.familyName` (`"Bedstead"`), `TeletextFont.fileName` (`"bedstead"`, the file `Resources/Fonts/bedstead.otf`), `sizeSteps` (`[15, 20, 25, 30]`), `defaultSize` (`20`), `isAvailable`, `snapped(_:)` (nearest step, ties up), `step(_:by:)` (next or previous step, clamped); `TeletextColor.allCases` and `.css` (`"#FF0000"`, upper case) in a test.
- Phase 4: `Appearance` (`.native`, `.antiope`), `Appearance.Keys.appearance`, `Appearance.Keys.crt` (`"antiope.crt"`), `Appearance.effective(stored:fontAvailable:)`.
- Phase 7: `PageDirectory(snapshot: SidebarSnapshot)` and `page(forFeed:) -> Int?`. `AppState.sidebar` is a `SidebarSnapshot` today, so the reader builds the directory from it.
- Phase 8: `AntiopeRootView` hosts the existing `ReaderView(state:)` in its reader column; nothing here depends on how.
- Phase 13 owns the CRT checkbox and the Increase contrast rule (spec §9). This phase only reads `@AppStorage(Appearance.Keys.crt)` and draws `data-crt` when the look is Antiope **and** the box is checked; it declares no `colorSchemeContrast` in `ReaderView`, because phase 13's Step 4 adds that property itself.

**Produces (pinned names in bold; the others are new public names, listed for the index):**
- `Settings/ReaderStyle.swift`: **`struct ReaderTheme: Equatable, Sendable { var appearance: Appearance; var crt: Bool }`** with `static let native` and `var showsCRT: Bool` (Antiope and `crt`); **`LineWidth.forty`** (`maxCh` 40, "40 columns"); **`ReaderStyle.bigger(_:appearance:)`**, **`ReaderStyle.smaller(_:appearance:)`**, plus `ReaderStyle.effectiveSize(_:appearance:)` (Native clamps to 12...28, Antiope snaps), `ReaderStyle.actualSize(for:)` (17 or 20, for ⌘0), `ReaderStyle.sizeKey(for:)` (`Keys.size` or `Keys.antiopeSize`), `ReaderStyle.storedSize(for:in:)`, `enum TextSizeCommand { case bigger, smaller, actualSize }` and `ReaderStyle.resize(_:appearance:in:)` (what ⌘+, ⌘− and ⌘0 do, on the look shown only).
- `Reading/ReaderView.swift`: **`struct SubpageLabel: Equatable, Sendable { var page: Int?; var index: Int; var count: Int }`** with `var text: String` ("412 3/20", or "3/20" without a page) and `static func of(entryId:in:page:) -> SubpageLabel?`; **`ArticlePage.theme: ReaderTheme`** and **`ArticlePage.subpage: SubpageLabel?`** (both defaulted, so existing call sites compile); `ReaderController.showSubpage(_:) async`. `ReaderView` builds its `ReaderTheme` in exactly one `ReaderTheme(` call, in a private `theme` property, with `crt: crt`: phase 13's Step 4 adds `\.colorSchemeContrast` and rewrites that `crt:` argument (checked: its edit applies to this file and compiles).
- `Reading/ArticleTemplate.swift`: `ArticleTemplate.page(for:fulltext:showFull:style:colorScheme:theme:subpage:)`, the last two defaulted to `.native` and `nil`. The page writes `data-theme="antiope"` and `data-crt` on `:root`, the custom property `--teletext-font`, the element `<p class="subpage" id="subpage">` and the title spans `dh-space` / `dh-ink`; phase 11 relies on these.

## Verified ahead of time (14/10/2026, this Mac, Xcode 27, macOS 27 SDK)

- **Code.** Every Swift block of this phase was compiled and its tests run in a SwiftPM harness (Swift 6.4, Swift 6 mode, complete strict concurrency, warnings as errors) with stand-ins for `TeletextFont`, `Appearance`, `PageDirectory`, `EntryRow`, `FulltextRow` and `AppState` in their pinned shapes, at both intermediate states (after 10a, after 10b). The edit blocks below were generated from those files and re-applied to today's sources by script, so each **Replace** block matches the current text exactly. The tests that read the app bundle (`readerCSS`, the Bedstead face) were run in the harness with the composed `reader.css` and `bedstead.otf` passed in by hand: green.
- **Rendering.** A full article page with this stylesheet and the real `bedstead.otf` (3.261) was rendered in a `WKWebView` and its snapshot inspected: every block lands on a 24 px row at 20 pt (sub-page line 24, meta 48, title 120 with height 96 for two double-height lines, byline 240, notice 288); `-webkit-font-smoothing: none` gives hard pixel edges; the `scaleY(2)` title is crisp at 15 and 20 pt on a Retina backing store; the CRT bleed draws on text only.
- **Double height for any number of lines.** `scaleY(2)` alone needs a bottom margin equal to the title's own height, which CSS cannot know for a title that wraps. The title is therefore written twice in one grid cell: a hidden copy (`visibility: hidden`, `aria-hidden`) at twice the row height gives the layout its height, the visible copy at one row is stretched over it. Verified with a two-line title.
- **`<mark>` never reaches the page.** The sanitiser's tidy pass is HTML 4 and unwraps `mark` (already noted in the mac-app phase 10). The `mark` rule below is kept for the day tidy changes; nothing tests it.

## Review Focus (this phase)

The index's five Review Focus items belong to other phases. Inputs this phase meets that its feature tests would not naturally hit:

1. **A title that wraps** (most titles at 40 columns): it must take exactly twice its line count in rows, with the body text on the next row. Pinned by the double-height markup test and the rendering check above.
2. **The list growing while an article is read** (a refresh brings new entries): "3/20" becomes "3/21" without reloading the page or losing the scroll position. `SubpageLineTests`.
3. **A feed that has no page number yet** (added after the last assignment): the line reads "3/20", never "nil 3/20". `SubpageLabelTests`.
4. **The selected entry filtered out of the list** (read while the list shows unread only): no label rather than a wrong position, and the line stays one row tall. `SubpageLabelTests` and `AntiopePageTests` (the empty line is still written).
5. **A stored size Antiope cannot draw (17, 28, NaN):** drawn at the nearest step, and ⌘+ / ⌘− move from what is drawn. `AntiopeReaderStyleTests`.

---

# Phase 10a

### Task 10a.0: Step 0, `ArticleTemplate.swift` cleanup (separate commit)

`ArticleTemplate.swift` is 453 lines, over the 300-line Step 0 threshold, and both phases restructure it. Dead weight first, behaviour unchanged.

**Files:**
- Modify: `apps/macos/Ressac/Reading/ArticleTemplate.swift`

**What goes:** nine members used only inside the file become `private` (`droppedElements`, `allowedElements`, `cacheCapacity`, `maxParsedBytes`, `maxInputBytes`, `safeURL`, `escapeText`, `escapeAttribute`, `contentSecurityPolicy`), and the duplicate `voidElements` set goes: the renderer only meets allowed elements, and the allowed void elements (`br col hr img wbr`) are exactly `voidTags` ∩ `allowedElements`, so `voidTags` serves both. `parseBudget`, `maxNestingDepth`, `estimatedNestingDepth`, `plainTextFallback`, `onLargeStack`, `cacheStats`, `readerCSS` and `fontFaces(for:)` stay internal: tests use them, and `parseBudget` is a default argument of an internal function. There are no debug logs and no unused imports (`Synchronization` is `Mutex`, `SwiftUI` is `ColorScheme`, `WebKit` is `WKNavigationType`).

- [ ] **Step 1: Check nothing outside the file uses the members that become private.**

```bash
cd /Users/willow/Sites/ressac   # branch antiope
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
grep -rnE "ArticleTemplate\.(droppedElements|allowedElements|cacheCapacity|maxParsedBytes|maxInputBytes|safeURL|escapeText|escapeAttribute|contentSecurityPolicy)" apps/macos
grep -rn "voidElements" apps/macos
```

Expected: the first command prints nothing; the second prints only the two lines of `ArticleTemplate.swift` (the declaration and its use in `render`). If a phase written after this plan uses one of these members from another file, leave that member internal and say so in the phase report.

- [ ] **Step 2: Record the baseline.** `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test 2>&1 | tail -5`. Expected: `** TEST SUCCEEDED **`. Note the number of tests Swift Testing reports (`Test run with N tests`).

- [ ] **Step 3: Apply the edits.**

**`apps/macos/Ressac/Reading/ArticleTemplate.swift`, edit 1.** Replace:

```swift
    /// Removed together with everything inside them.
    static let droppedElements: Set<String> = [
        "applet", "audio", "base", "button", "canvas", "dialog", "embed", "form", "frame", "frameset",
        "head", "iframe", "input", "link", "math", "meta", "noscript", "object", "option", "script",
        "select", "source", "style", "svg", "template", "textarea", "title", "track", "video",
    ]

    /// Kept, with their allowed attributes. Any other element is unwrapped: its children stay.
    static let allowedElements: Set<String> = [
```

with:

```swift
    /// Removed together with everything inside them.
    private static let droppedElements: Set<String> = [
        "applet", "audio", "base", "button", "canvas", "dialog", "embed", "form", "frame", "frameset",
        "head", "iframe", "input", "link", "math", "meta", "noscript", "object", "option", "script",
        "select", "source", "style", "svg", "template", "textarea", "title", "track", "video",
    ]

    /// Kept, with their allowed attributes. Any other element is unwrapped: its children stay.
    private static let allowedElements: Set<String> = [
```

**`apps/macos/Ressac/Reading/ArticleTemplate.swift`, edit 2.** Replace:

```swift

    private static let voidElements: Set<String> = ["br", "col", "hr", "img", "wbr"]
    private static let globalAttributes: Set<String> = ["dir", "lang", "title"]
```

with:

```swift

    private static let globalAttributes: Set<String> = ["dir", "lang", "title"]
```

**`apps/macos/Ressac/Reading/ArticleTemplate.swift`, edit 3.** Replace:

```swift

    static let cacheCapacity = 32
    private static let cache = Mutex(Memo())
```

with:

```swift

    private static let cacheCapacity = 32
    private static let cache = Mutex(Memo())
```

**`apps/macos/Ressac/Reading/ArticleTemplate.swift`, edit 4.** Replace:

```swift
    static let parseBudget: TimeInterval = 1.0
    static let maxParsedBytes = 256 * 1024
    static let maxNestingDepth = 256
    static let maxInputBytes = 2_000_000
```

with:

```swift
    static let parseBudget: TimeInterval = 1.0
    private static let maxParsedBytes = 256 * 1024
    static let maxNestingDepth = 256
    private static let maxInputBytes = 2_000_000
    /// HTML's void elements: never closed, never nested (the renderer and the depth estimate).
```

**`apps/macos/Ressac/Reading/ArticleTemplate.swift`, edit 5.** Replace:

```swift
            out += ">"
            guard !voidElements.contains(tag) else { return }
```

with:

```swift
            out += ">"
            guard !voidTags.contains(tag) else { return }
```

**`apps/macos/Ressac/Reading/ArticleTemplate.swift`, edit 6.** Replace:

```swift

    static func safeURL(_ raw: String, baseURL: URL?, allowImageData: Bool) -> String? {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines.union(.controlCharacters))
```

with:

```swift

    private static func safeURL(_ raw: String, baseURL: URL?, allowImageData: Bool) -> String? {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines.union(.controlCharacters))
```

**`apps/macos/Ressac/Reading/ArticleTemplate.swift`, edit 7.** Replace:

```swift

    static func escapeText(_ text: String) -> String {
        text.replacingOccurrences(of: "&", with: "&amp;")
            .replacingOccurrences(of: "<", with: "&lt;")
            .replacingOccurrences(of: ">", with: "&gt;")
    }

    static func escapeAttribute(_ text: String) -> String {
        escapeText(text).replacingOccurrences(of: "\"", with: "&quot;")
    }

    // MARK: Page

    /// No scripts, frames, objects, forms or base URL; images from the web; fonts and styles inline.
    static let contentSecurityPolicy =
        "default-src 'none'; img-src http: https: data:; style-src 'unsafe-inline'; font-src data:; base-uri 'none'; form-action 'none'"
```

with:

```swift

    private static func escapeText(_ text: String) -> String {
        text.replacingOccurrences(of: "&", with: "&amp;")
            .replacingOccurrences(of: "<", with: "&lt;")
            .replacingOccurrences(of: ">", with: "&gt;")
    }

    private static func escapeAttribute(_ text: String) -> String {
        escapeText(text).replacingOccurrences(of: "\"", with: "&quot;")
    }

    // MARK: Page

    /// No scripts, frames, objects, forms or base URL; images from the web; fonts and styles inline.
    private static let contentSecurityPolicy =
        "default-src 'none'; img-src http: https: data:; style-src 'unsafe-inline'; font-src data:; base-uri 'none'; form-action 'none'"
```

- [ ] **Step 4: Run the same command.** Expected: `** TEST SUCCEEDED **`, the same `N` tests, zero warnings. A compile error naming one of the nine members means Step 1 missed a use: put that member back to internal.

- [ ] **Step 5: Commit.**

```bash
git add apps/macos/Ressac/Reading/ArticleTemplate.swift
git commit -m "ArticleTemplate: Step 0, file-private helpers and one void-tag set

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 10a.1: `ReaderStyle`: 40 columns, the reader theme, size steps per look

**Files:**
- Modify: `apps/macos/Ressac/Settings/ReaderStyle.swift`
- Test: `apps/macos/RessacTests/ReaderTests.swift`

**Interfaces:**
- Consumes: `Appearance` (phase 4); `TeletextFont.snapped(_:)`, `step(_:by:)`, `defaultSize` (phase 3).
- Produces: `LineWidth.forty`; `ReaderTheme` (`appearance`, `crt`, `static let native`, `showsCRT`); `ReaderStyle.effectiveSize(_:appearance:)`, `bigger(_:appearance:)`, `smaller(_:appearance:)`, `actualSize(for:)`, `sizeKey(for:)`, `storedSize(for:in:)`, `resize(_:appearance:in:)`; `TextSizeCommand`.

Rule (spec §5, decision of 2026-10-14): each look has **its own stored size**. Native: `Keys.size`, 12...28 by 1 pt, default 17. Antiope: `Keys.antiopeSize` (phase 3), default `TeletextFont.defaultSize` (20), always one of 15, 20, 25, 30 once the menu or Settings wrote it. ⌘+, ⌘− and ⌘0 act on the size of the look shown and never touch the other one, so switching looks changes no size; ⌘0 gives 17 in Native and 20 in Antiope. Antiope draws `TeletextFont.snapped(size)` and steps from **what it draws**: a hand-edited 17 is drawn at 15, so ⌘+ gives 20 and ⌘− stays at 15. A non-finite stored value is drawn at the default, 20.

- [ ] **Step 1: Write the failing tests.** In `ReaderTests.swift`, the existing `defaultsMatchTheSpec` lists the widths; it gains the new one:

**`apps/macos/RessacTests/ReaderTests.swift`, edit 1.** Replace:

```swift
        #expect(style.width == .medium)
        #expect(LineWidth.allCases.map(\.maxCh) == [58, 68, 80])
```

with:

```swift
        #expect(style.width == .medium)
        #expect(LineWidth.allCases.map(\.maxCh) == [58, 68, 80, 40])
```

Then append to the end of `ReaderTests.swift`:

```swift
@Suite struct AntiopeReaderStyleTests {
    @Test func antiopeStepsThroughBedsteadSizes() {
        #expect(ReaderStyle.effectiveSize(17, appearance: .antiope) == 15)
        #expect(ReaderStyle.effectiveSize(17, appearance: .native) == 17)
        #expect(ReaderStyle.effectiveSize(.nan, appearance: .antiope) == 20)
        #expect(ReaderStyle.effectiveSize(40, appearance: .native) == 28)
        #expect(ReaderStyle.bigger(17, appearance: .antiope) == 20)  // drawn at 15, so the next step is 20
        #expect(ReaderStyle.smaller(17, appearance: .antiope) == 15)
        #expect(ReaderStyle.bigger(20, appearance: .antiope) == 25)
        #expect(ReaderStyle.bigger(30, appearance: .antiope) == 30)
        #expect(ReaderStyle.smaller(25, appearance: .antiope) == 20)
        #expect(ReaderStyle.smaller(15, appearance: .antiope) == 15)
        #expect(ReaderStyle.bigger(17, appearance: .native) == 18)
        #expect(ReaderStyle.smaller(17, appearance: .native) == 16)
        #expect(ReaderStyle.actualSize(for: .antiope) == 20)
        #expect(ReaderStyle.actualSize(for: .native) == 17)
    }

    @Test func fortyColumnsAndTheTheme() {
        #expect(LineWidth.forty.maxCh == 40)
        #expect(LineWidth.forty.displayName == "40 columns")
        #expect(LineWidth.allCases.map(\.maxCh) == [58, 68, 80, 40])
        #expect(ReaderTheme.native == ReaderTheme(appearance: .native, crt: false))
        #expect(!ReaderTheme(appearance: .native, crt: true).showsCRT)
        #expect(!ReaderTheme(appearance: .antiope, crt: false).showsCRT)
        #expect(ReaderTheme(appearance: .antiope, crt: true).showsCRT)
    }
}

/// Decision of 2026-10-14: each look keeps its own text size, and ⌘+, ⌘− and ⌘0 act on the
/// look shown. Every test writes a throwaway defaults suite: the test host's are Clément's.
@Suite struct TextSizePerLookTests {
    private func withDefaults(_ body: (UserDefaults) throws -> Void) throws {
        let suite = "ressac.tests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        try body(defaults)
    }

    @Test func switchingLooksNeverChangesTheOtherSize() throws {
        #expect(ReaderStyle.sizeKey(for: .native) == "reader.size")
        #expect(ReaderStyle.sizeKey(for: .antiope) == "reader.size.antiope")
        try withDefaults { defaults in
            #expect(ReaderStyle.storedSize(for: .native, in: defaults) == 17)
            #expect(ReaderStyle.storedSize(for: .antiope, in: defaults) == 20)  // not Native's 17 snapped to 15
            ReaderStyle.resize(.bigger, appearance: .antiope, in: defaults)
            #expect(defaults.double(forKey: ReaderStyle.Keys.antiopeSize) == 25)
            #expect(defaults.object(forKey: ReaderStyle.Keys.size) == nil)  // Native is found as it was
            ReaderStyle.resize(.smaller, appearance: .native, in: defaults)
            #expect(defaults.double(forKey: ReaderStyle.Keys.size) == 16)
            #expect(defaults.double(forKey: ReaderStyle.Keys.antiopeSize) == 25)
        }
    }

    @Test func actualSizeRestoresTheDefaultOfTheLookShown() throws {
        try withDefaults { defaults in
            defaults.set(22.0, forKey: ReaderStyle.Keys.size)
            defaults.set(30.0, forKey: ReaderStyle.Keys.antiopeSize)
            ReaderStyle.resize(.actualSize, appearance: .antiope, in: defaults)
            #expect(defaults.double(forKey: ReaderStyle.Keys.antiopeSize) == 20)
            #expect(defaults.double(forKey: ReaderStyle.Keys.size) == 22)
            ReaderStyle.resize(.actualSize, appearance: .native, in: defaults)
            #expect(defaults.double(forKey: ReaderStyle.Keys.size) == 17)
            #expect(defaults.double(forKey: ReaderStyle.Keys.antiopeSize) == 20)
        }
    }

    /// Whatever is stored (a hand edit, an older build), Antiope draws a step and the menu
    /// stores a step.
    @Test func theAntiopeSizeIsAlwaysAStep() throws {
        try withDefaults { defaults in
            for stored in [17.0, 0, -3, 22.5, 99] {
                #expect(TeletextFont.sizeSteps.contains(ReaderStyle.effectiveSize(stored, appearance: .antiope)), "\(stored)")
                for command in [TextSizeCommand.bigger, .smaller, .actualSize] {
                    defaults.set(stored, forKey: ReaderStyle.Keys.antiopeSize)
                    ReaderStyle.resize(command, appearance: .antiope, in: defaults)
                    #expect(TeletextFont.sizeSteps.contains(defaults.double(forKey: ReaderStyle.Keys.antiopeSize)),
                            "\(stored) \(command)")
                }
            }
        }
    }
}
```

- [ ] **Step 2: Run red.** `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/AntiopeReaderStyleTests -only-testing:RessacTests/TextSizePerLookTests -only-testing:RessacTests/ReaderStyleTests`. Expected: the build fails with `type 'ReaderStyle' has no member 'effectiveSize'`, `type 'LineWidth' has no member 'forty'`, `cannot find 'ReaderTheme' in scope` and `cannot find 'TextSizeCommand' in scope`.

- [ ] **Step 3: Implement.** In `ReaderStyle.swift`:

**`apps/macos/Ressac/Settings/ReaderStyle.swift`, edit 1.** Replace:

```swift
    case narrow, medium, wide
```

with:

```swift
    case narrow, medium, wide
    /// Exactly 40 cells in Antiope, where `1ch` is one Bedstead cell (spec §5); offered in both looks.
    case forty
```

**`apps/macos/Ressac/Settings/ReaderStyle.swift`, edit 2.** Replace:

```swift
        case .wide: 80
```

with:

```swift
        case .wide: 80
        case .forty: 40
```

**`apps/macos/Ressac/Settings/ReaderStyle.swift`, edit 3.** Replace:

```swift
        case .wide: "Wide"
        }
    }
```

with:

```swift
        case .wide: "Wide"
        case .forty: "40 columns"
        }
    }
}

/// The look the reader draws in (spec §5 and §9).
struct ReaderTheme: Equatable, Sendable {
    var appearance: Appearance
    /// The CRT screen box (`Appearance.Keys.crt`); phase 13 also passes it off under Increase contrast.
    var crt: Bool

    static let native = ReaderTheme(appearance: .native, crt: false)

    /// The colour bleed applies only to Antiope with the box checked (spec §9).
    var showsCRT: Bool { appearance == .antiope && crt }
```

**`apps/macos/Ressac/Settings/ReaderStyle.swift`, edit 4.** Replace:

```swift
        return style
    }
}
```

with:

```swift
        return style
    }
}

/// What View ▸ Bigger Text (⌘+), Smaller Text (⌘−) and Actual Size (⌘0) ask for.
enum TextSizeCommand: Sendable {
    case bigger, smaller, actualSize
}

/// Text size per look (spec §5): Native moves by 1 pt in 12...28; Antiope draws only Bedstead's
/// steps (15, 20, 25, 30), so one font pixel lands on whole Retina pixels. Each look has its own
/// stored size (decision of 2026-10-14); Antiope snaps its size when drawing and steps from what
/// it draws.
extension ReaderStyle {
    static func effectiveSize(_ size: Double, appearance: Appearance) -> Double {
        switch appearance {
        case .native: clampedSize(size)
        case .antiope: TeletextFont.snapped(size.isFinite ? size : TeletextFont.defaultSize)
        }
    }

    static func bigger(_ size: Double, appearance: Appearance) -> Double {
        switch appearance {
        case .native: bigger(size)
        case .antiope: TeletextFont.step(effectiveSize(size, appearance: .antiope), by: 1)
        }
    }

    static func smaller(_ size: Double, appearance: Appearance) -> Double {
        switch appearance {
        case .native: smaller(size)
        case .antiope: TeletextFont.step(effectiveSize(size, appearance: .antiope), by: -1)
        }
    }

    /// What View ▸ Actual Size (⌘0) restores, and each look's size before any change.
    static func actualSize(for appearance: Appearance) -> Double {
        switch appearance {
        case .native: defaultSize
        case .antiope: TeletextFont.defaultSize
        }
    }

    /// Where each look stores its size: switching looks never changes the other one.
    static func sizeKey(for appearance: Appearance) -> String {
        switch appearance {
        case .native: Keys.size
        case .antiope: Keys.antiopeSize
        }
    }

    /// The size `appearance` stores, or its default while none is stored.
    static func storedSize(for appearance: Appearance, in defaults: UserDefaults) -> Double {
        let key = sizeKey(for: appearance)
        return defaults.object(forKey: key) == nil ? actualSize(for: appearance) : defaults.double(forKey: key)
    }

    /// ⌘+, ⌘− or ⌘0 on the size of the look shown, and only on it.
    static func resize(_ command: TextSizeCommand, appearance: Appearance, in defaults: UserDefaults) {
        let size = storedSize(for: appearance, in: defaults)
        let resized = switch command {
        case .bigger: bigger(size, appearance: appearance)
        case .smaller: smaller(size, appearance: appearance)
        case .actualSize: actualSize(for: appearance)
        }
        defaults.set(resized, forKey: sizeKey(for: appearance))
    }
}
```

**`apps/macos/Ressac/Settings/ReaderStyle.swift`, edit 5.** Replace:

```swift
/// How articles look. Persisted with `@AppStorage` under `Keys`; Phase 9's View menu
/// (Bigger Text ⌘+, Smaller Text ⌘−, Actual Size ⌘0) writes `Keys.size` directly.
```

with:

```swift
/// How articles look. Persisted with `@AppStorage` under `Keys`; the View menu (Bigger Text ⌘+,
/// Smaller Text ⌘−, Actual Size ⌘0) changes the size of the look shown, `resize(_:appearance:in:)`.
```

- [ ] **Step 4: Run green.** Same command. Expected: `AntiopeReaderStyleTests` (2 tests), `TextSizePerLookTests` (3 tests) and `ReaderStyleTests` (4 tests) pass.

- [ ] **Step 5: Commit.**

```bash
git add apps/macos/Ressac/Settings/ReaderStyle.swift apps/macos/RessacTests/ReaderTests.swift
git commit -m "Reader style: 40 columns, reader theme, Bedstead size steps in Antiope

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 10a.2: ⌘+ / ⌘− / ⌘0 and the Settings slider follow the look

**Files:**
- Modify: `apps/macos/Ressac/App/Commands.swift`
- Modify: `apps/macos/Ressac/Settings/SettingsView.swift`

**Interfaces:**
- Consumes: `ReaderStyle.resize(_:appearance:in:)`, `effectiveSize(_:appearance:)`, `Keys.size` (Task 10a.1), `Keys.antiopeSize` (phase 3); `Appearance.Keys.appearance`, `Appearance.effective(stored:fontAvailable:)`; `TeletextFont.isAvailable`, `sizeSteps`, `defaultSize`.
- Produces: no new name outside these files (`TextSizeMenuItems` and `TextSizeRow` are `private`).

`TextSizeMenuItems` hard-codes `"reader.size"`, `17`, `12` and `28`, a second copy of `ReaderStyle`'s rules; it now calls `ReaderStyle.resize(_:appearance:in:)` on the standard defaults (where `@AppStorage` reads outside a test), which picks the look's own key: ⌘+ in Antiope goes 20 → 25 → 30 on `reader.size.antiope` and leaves the Native size alone, and ⌘0 gives 20 there, 17 in Native. In Settings, the text size row moves into its own view that reads the appearance itself and binds the size of the look shown: in Antiope its slider has four positions (the index into `TeletextFont.sizeSteps`) over `reader.size.antiope` and the label shows the drawn size. This leaves `ReadingSettings` free for phase 4's Appearance picker and phase 13's CRT box, whatever names they gave their own properties. The line width picker lists `LineWidth.allCases`, so "40 columns" appears there with no change.

No new unit test: both views are thin wrappers over the functions Task 10a.1 tests (`TextSizePerLookTests` pins the per-look keys, ⌘0 per look and the steps). The build and the full suite are the check.

- [ ] **Step 1: The menu.** In `Commands.swift`:

**`apps/macos/Ressac/App/Commands.swift`, edit 1.** Replace:

```swift

/// Reader text size in points (12...28, default 17). The key is phase 10's `ReaderStyle.Keys.size`.
private struct TextSizeMenuItems: View {
    @AppStorage("reader.size") private var textSize = 17.0

    var body: some View {
        Button("Bigger Text") { textSize = min(28, textSize + 1) }
            .keyboardShortcut("+")
        Button("Smaller Text") { textSize = max(12, textSize - 1) }
            .keyboardShortcut("-")
        Button("Actual Size") { textSize = 17 }
            .keyboardShortcut("0")
```

with:

```swift

/// Reader text size (spec §5): 12...28 by 1 pt in Native, Bedstead's steps in Antiope. Each look
/// has its own stored size (`ReaderStyle.sizeKey(for:)`); the commands act on the look shown.
/// The standard defaults are the store `@AppStorage` reads everywhere outside the tests.
private struct TextSizeMenuItems: View {
    @AppStorage(Appearance.Keys.appearance) private var storedAppearance = Appearance.native

    private var appearance: Appearance {
        Appearance.effective(stored: storedAppearance, fontAvailable: TeletextFont.isAvailable)
    }

    var body: some View {
        Button("Bigger Text") { ReaderStyle.resize(.bigger, appearance: appearance, in: .standard) }
            .keyboardShortcut("+")
        Button("Smaller Text") { ReaderStyle.resize(.smaller, appearance: appearance, in: .standard) }
            .keyboardShortcut("-")
        Button("Actual Size") { ReaderStyle.resize(.actualSize, appearance: appearance, in: .standard) }
            .keyboardShortcut("0")
```

- [ ] **Step 2: Settings.** In `SettingsView.swift`, the text size row becomes `TextSizeRow()`, declared under `ReadingSettings`. If phase 4 already edited the `LabeledContent("Text size:")` block (for example to disable it), replace that whole block, as it stands, by `TextSizeRow()` and keep phase 4's other changes.

**`apps/macos/Ressac/Settings/SettingsView.swift`, edit 1.** Replace:

```swift
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
```

with:

```swift
            TextSizeRow()
```

**`apps/macos/Ressac/Settings/SettingsView.swift`, edit 2.** Replace:

```swift
private struct DataSettings: View {
```

with:

```swift
/// The text size row of Settings ▸ Reading: a 1 pt slider over the Native size in Native, a
/// four-step slider over Antiope's own size in Antiope (spec §5, decision of 2026-10-14). It
/// reads the appearance itself, so it does not depend on how `ReadingSettings` holds it.
private struct TextSizeRow: View {
    @AppStorage(ReaderStyle.Keys.size) private var size = ReaderStyle.defaultSize
    @AppStorage(ReaderStyle.Keys.antiopeSize) private var antiopeSize = TeletextFont.defaultSize
    @AppStorage(Appearance.Keys.appearance) private var storedAppearance = Appearance.native

    private var appearance: Appearance {
        Appearance.effective(stored: storedAppearance, fontAvailable: TeletextFont.isAvailable)
    }

    /// The size the look shown draws at.
    private var shownSize: Double {
        ReaderStyle.effectiveSize(appearance == .antiope ? antiopeSize : size, appearance: appearance)
    }

    var body: some View {
        LabeledContent("Text size:") {
            HStack(spacing: 8) {
                slider
                    .labelsHidden()
                Text("\(Int(shownSize)) pt")
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .frame(width: 40, alignment: .trailing)
            }
        }
    }

    @ViewBuilder
    private var slider: some View {
        switch appearance {
        case .native:
            Slider(value: $size, in: ReaderStyle.sizeRange, step: ReaderStyle.sizeStep) {
                Text("Text size")
            } minimumValueLabel: {
                Image(systemName: "textformat.size.smaller")
            } maximumValueLabel: {
                Image(systemName: "textformat.size.larger")
            }
        case .antiope:
            Slider(value: teletextStep, in: 0...Double(TeletextFont.sizeSteps.count - 1), step: 1) {
                Text("Text size")
            } minimumValueLabel: {
                Image(systemName: "textformat.size.smaller")
            } maximumValueLabel: {
                Image(systemName: "textformat.size.larger")
            }
        }
    }

    /// The index of Antiope's drawn size in `TeletextFont.sizeSteps`; moving it stores that step.
    private var teletextStep: Binding<Double> {
        Binding(
            get: { Double(TeletextFont.sizeSteps.firstIndex(of: ReaderStyle.effectiveSize(antiopeSize, appearance: .antiope)) ?? 0) },
            set: { index in
                let steps = TeletextFont.sizeSteps
                antiopeSize = steps[min(max(Int(index.rounded()), 0), steps.count - 1)]
            })
    }
}

private struct DataSettings: View {
```

- [ ] **Step 3: Verification gate (phase 10a).**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test
```

Expected: every command succeeds; `** TEST SUCCEEDED **`; zero warnings in our sources (`SWIFT_TREAT_WARNINGS_AS_ERRORS` would fail the build otherwise). There is no `tsc` or `eslint` in this project.

- [ ] **Step 4: Commit and push.**

```bash
git add apps/macos/Ressac/App/Commands.swift apps/macos/Ressac/Settings/SettingsView.swift
git commit -m "Text size menu and Settings slider step through Bedstead sizes in Antiope

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
git -c credential.helper= -c credential.helper='!gh auth git-credential' push
```

**Stop here for Clément's approval before phase 10b.**

---

# Phase 10b

### Task 10b.1: `SubpageLabel`

**Files:**
- Modify: `apps/macos/Ressac/Reading/ReaderView.swift`
- Test: `apps/macos/RessacTests/ReaderTests.swift`

**Interfaces:**
- Consumes: `EntryRow.entryId`, `EntryRow.feedId`.
- Produces: `SubpageLabel` (`page`, `index`, `count`, `text`, `static func of(entryId:in:page:)`). `page` is a closure, not a `PageDirectory`, so the test needs no sidebar snapshot; the reader passes `directory.page(forFeed:)`.

Spec §4, Sub-pages: the position is counted in the list currently shown (feed, folder or smart list, with its filter and sort), numbered from 1; the page part is always the entry's own feed page, even in Unread. An entry no longer in the list (read while the list shows unread only, then refiltered) gives no label rather than a wrong one.

- [ ] **Step 1: Write the failing test.** Append to `ReaderTests.swift`:

```swift
@Suite struct SubpageLabelTests {
    @Test func theLabelReadsPageIndexAndCount() {
        #expect(SubpageLabel(page: 412, index: 3, count: 20).text == "412 3/20")
        #expect(SubpageLabel(page: nil, index: 1, count: 1).text == "1/1")
        let list = [entry(id: 5), entry(id: 6), entry(id: 7)]
        #expect(SubpageLabel.of(entryId: 6, in: list) { $0 == 7 ? 412 : nil } == SubpageLabel(page: 412, index: 2, count: 3))
        #expect(SubpageLabel.of(entryId: 9, in: list) { _ in 412 } == nil)
    }
}
```

- [ ] **Step 2: Run red.** `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/SubpageLabelTests`. Expected: build failure, `cannot find 'SubpageLabel' in scope`.

- [ ] **Step 3: Implement.** In `ReaderView.swift`:

**`apps/macos/Ressac/Reading/ReaderView.swift`, edit 1.** Replace:

```swift
            : "Show the full article, fetched from the site"
```

with:

```swift
            : "Show the full article, fetched from the site"
    }
}

/// An entry's place in the list shown, under its own feed's page (spec §4, Sub-pages): "412 3/20".
struct SubpageLabel: Equatable, Sendable {
    var page: Int?
    var index: Int
    var count: Int

    /// "412 3/20", or "3/20" for a feed without a page number yet.
    var text: String { (page.map { "\($0) " } ?? "") + "\(index)/\(count)" }

    /// Numbered from 1 in `entries`; nil when the entry is not in the list (filtered out).
    static func of(entryId: Int64, in entries: [EntryRow], page: (Int64) -> Int?) -> SubpageLabel? {
        guard let position = entries.firstIndex(where: { $0.entryId == entryId }) else { return nil }
        return SubpageLabel(page: page(entries[position].feedId), index: position + 1, count: entries.count)
```

- [ ] **Step 4: Run green.** Same command. Expected: 1 test passes.

- [ ] **Step 5: Commit.**

```bash
git add apps/macos/Ressac/Reading/ReaderView.swift apps/macos/RessacTests/ReaderTests.swift
git commit -m "Reader: sub-page label (412 3/20)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 10b.2: The Antiope page and stylesheet

**Files:**
- Modify: `apps/macos/Ressac/Reading/ArticleTemplate.swift`
- Modify: `apps/macos/Ressac/Resources/reader.css`
- Test: `apps/macos/RessacTests/ReaderTests.swift`

**Interfaces:**
- Consumes: `ReaderTheme`, `ReaderStyle.effectiveSize(_:appearance:)` (10a.1); `SubpageLabel.text` (10b.1); `TeletextFont.fileName`, `familyName`; `TeletextColor` (test).
- Produces: `ArticleTemplate.page(for:fulltext:showFull:style:colorScheme:theme:subpage:)` (`theme` defaults to `.native`, `subpage` to `nil`: every existing call and test keeps its output); in Antiope the page carries `data-theme="antiope"`, `data-crt` when `theme.showsCRT`, `--teletext-font`, Bedstead's `@font-face`, `<p class="subpage" id="subpage">` (always present, empty when the entry is not in the list, so 10b.3 can fill it in place) and the double-height title. `reader.css` gains two sections, delimited by comments the tests look for: `Antiope begin` … `Antiope end`, then `CRT begin` … `CRT end`.

**Page rules (Antiope only; Native output is unchanged byte for byte):**
- `:root` attributes: `lang="en" data-scheme="…" data-theme="antiope"`, plus ` data-crt` when the theme shows the CRT.
- Fonts: Bedstead's `@font-face` replaces the chosen reader font's faces (the web content process cannot see fonts the app registers, notes 2026-10-10); `--teletext-font: "Bedstead", monospace` is set inline and the stylesheet points `--reader-font`, `--ui-font` and `--mono-font` at it. The face is built once per process, like Newsreader's (about 290 KB of base64 for the 218 KB `bedstead.otf`).
- Size: `--reader-size` is `ReaderStyle.effectiveSize(style.size, appearance:)`, so 17 is written as 15 px.
- Header: the sub-page line, the meta line, then the title written twice (hidden spacer copy, visible copy carrying the link).

**Stylesheet rules (spec §7, §9):** the Antiope section redefines every colour variable of the two native palettes with the eight teletext colours, so no native colour can leak; `-webkit-font-smoothing: none`, no kerning, ligatures or synthetic bold and italic; one size and one weight everywhere; every vertical margin a whole number of 1.2 em rows (12 font pixels); yellow double-height title, cyan meta line and links without underline, green `h2` and code, cyan `h3`, a magenta bar half a cell wide on blockquotes (the left sextant column), black on yellow `mark`, black on red notice, a blue rule one font pixel thick. The CRT section adds the bleed on text: red at +1 px and blue at −1 px at opacity 0.45, plus a 2 px glow of the text's own colour; `text-shadow` never touches images, so mosaics and originals get none.

- [ ] **Step 1: Write the failing tests.** Append to `ReaderTests.swift`. (The page embeds `reader.css`, whose text names the Antiope selectors, so these tests check the emitted markup itself rather than the absence of a word.)

```swift
@Suite struct AntiopePageTests {
    private let antiope = ReaderTheme(appearance: .antiope, crt: false)

    private func page(theme: ReaderTheme, subpage: SubpageLabel? = nil, style: ReaderStyle = ReaderStyle(), title: String? = "A title") -> String {
        ArticleTemplate.page(for: entry(title: title), fulltext: nil, showFull: false, style: style, colorScheme: .dark,
                             theme: theme, subpage: subpage)
    }

    @Test func nativeStaysAsItWas() {
        // The page embeds reader.css, whose text names the Antiope selectors: check the markup itself.
        let native = page(theme: .native, subpage: SubpageLabel(page: 412, index: 3, count: 20))
        #expect(native.contains(#"<html lang="en" data-scheme="dark">"#))
        #expect(!native.contains(#"<p class="subpage""#))
        #expect(!native.contains(#"--teletext-font: "Bedstead""#))
        #expect(native.contains(#"<h1 class="title"><a href="https://blog.example.org/2026/10/post.html">A title</a></h1>"#))
    }

    @Test func antiopeMarksTheRootSnapsTheSizeAndDoublesTheTitle() {
        let html = page(theme: antiope, subpage: SubpageLabel(page: 412, index: 3, count: 20),
                        style: ReaderStyle(font: .charter, size: 17, width: .forty))
        #expect(html.contains(#"<html lang="en" data-scheme="dark" data-theme="antiope">"#))
        #expect(html.contains("--reader-size: 15px"))
        #expect(html.contains("--reader-measure: 40ch"))
        #expect(html.contains(#"--teletext-font: "Bedstead", monospace;"#))
        #expect(html.contains(#"<p class="subpage" id="subpage">412 3/20</p>"#))
        #expect(html.contains(#"<h1 class="title"><span class="dh-space" aria-hidden="true">A title</span><span class="dh-ink"><a href="https://blog.example.org/2026/10/post.html">A title</a></span></h1>"#))
        // The sub-page line exists even before the list knows the entry, so it can be filled in place.
        #expect(page(theme: antiope).contains(#"<p class="subpage" id="subpage"></p>"#))
        #expect(page(theme: antiope, title: "<b>&</b>").contains(#"<span class="dh-space" aria-hidden="true">&lt;b&gt;&amp;&lt;/b&gt;</span>"#))
    }

    @Test func dataCRTOnlyForAntiopeWithTheBoxChecked() {
        #expect(page(theme: ReaderTheme(appearance: .native, crt: true)).contains(#"<html lang="en" data-scheme="dark">"#))
        #expect(page(theme: antiope).contains(#"<html lang="en" data-scheme="dark" data-theme="antiope">"#))
        #expect(page(theme: ReaderTheme(appearance: .antiope, crt: true))
            .contains(#"<html lang="en" data-scheme="dark" data-theme="antiope" data-crt>"#))
    }
}

/// Spec §5: no colour but the eight in an Antiope view; §9: the bleed is text-only, under data-crt.
@Suite struct AntiopeStylesheetTests {
    @Test func theAntiopeSectionUsesOnlyTheEightColours() throws {
        let css = ArticleTemplate.readerCSS
        let start = try #require(css.range(of: "Antiope begin"))
        let end = try #require(css.range(of: "Antiope end"))
        let section = String(css[start.upperBound..<end.lowerBound])
        let colours = Set(section.matches(of: /#[0-9A-Fa-f]{3,8}\b/).map { String($0.output).uppercased() })
        #expect(colours == Set(TeletextColor.allCases.map(\.css)))
        for function in ["rgb(", "rgba(", "hsl(", "hsla("] {
            #expect(!section.contains(function), "\(function)")
        }
        #expect(section.contains("--smoothing: none;"))
        #expect(section.contains("transform: scaleY(2);"))
    }

    @Test func theCRTBleedIsTextOnlyAndOnlyUnderDataCRT() {
        #expect(ArticleTemplate.readerCSS.contains(#":root[data-theme="antiope"][data-crt] body {"#
            + "\n  text-shadow: 1px 0 rgba(255, 0, 0, 0.45), -1px 0 rgba(0, 0, 255, 0.45), 0 0 2px currentColor;"))
    }

    @Test func bedsteadIsEmbeddedInAntiopeOnly() {
        let face = #"@font-face { font-family: "Bedstead"; font-style: normal; font-weight: 400; src: url("data:font/otf;base64,"#
        let antiope = ArticleTemplate.page(for: entry(), fulltext: nil, showFull: false, style: ReaderStyle(font: .newsreader),
                                           colorScheme: .light, theme: ReaderTheme(appearance: .antiope, crt: false))
        let native = ArticleTemplate.page(for: entry(), fulltext: nil, showFull: false, style: ReaderStyle(font: .newsreader),
                                          colorScheme: .light)
        #expect(antiope.contains(face))
        #expect(!antiope.contains(#"font-family: "Newsreader""#))
        #expect(!native.contains(face))
    }
}
```

- [ ] **Step 2: Run red.** `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/AntiopePageTests -only-testing:RessacTests/AntiopeStylesheetTests`. Expected: build failure, `extra arguments at positions #6, #7 in call` on `ArticleTemplate.page`.

- [ ] **Step 3: Implement the page.** In `ArticleTemplate.swift`:

**`apps/macos/Ressac/Reading/ArticleTemplate.swift`, edit 1.** Replace:

```swift

    static func page(for entry: EntryRow, fulltext: FulltextRow?, showFull: Bool, style: ReaderStyle, colorScheme: ColorScheme) -> String {
        let base = LinkPolicy.webURL(entry.url)
```

with:

```swift

    static func page(for entry: EntryRow, fulltext: FulltextRow?, showFull: Bool, style: ReaderStyle, colorScheme: ColorScheme,
                     theme: ReaderTheme = .native, subpage: SubpageLabel? = nil) -> String {
        let antiope = theme.appearance == .antiope
        let base = LinkPolicy.webURL(entry.url)
```

**`apps/macos/Ressac/Reading/ArticleTemplate.swift`, edit 2.** Replace:

```swift
        let emptyText = base == nil ? "This entry has no text." : "This entry has no text. Open it in the browser to read it."

        return """
        <!doctype html>
        <html lang="en" data-scheme="\(colorScheme == .dark ? "dark" : "light")">
        <head>
        <meta charset="utf-8">
        <meta http-equiv="Content-Security-Policy" content="\(contentSecurityPolicy)">
        <meta name="referrer" content="no-referrer">
        <style>
        \(fontFaces(for: style.font))
        \(readerCSS)
        :root { --reader-font: \(style.font.cssFamily); --reader-size: \(Int(ReaderStyle.clampedSize(style.size)))px; --reader-measure: \(style.width.maxCh)ch; }
        </style>
        </head>
        <body>
        <article>
        <p class="meta">\(meta.joined(separator: #"<span class="dot">·</span>"#))</p>
        <h1 class="title">\(heading)</h1>
```

with:

```swift
        let emptyText = base == nil ? "This entry has no text." : "This entry has no text. Open it in the browser to read it."
        let size = ReaderStyle.effectiveSize(style.size, appearance: theme.appearance)

        return """
        <!doctype html>
        <html \(rootAttributes(colorScheme: colorScheme, theme: theme))>
        <head>
        <meta charset="utf-8">
        <meta http-equiv="Content-Security-Policy" content="\(contentSecurityPolicy)">
        <meta name="referrer" content="no-referrer">
        <style>
        \(antiope ? teletextFace : fontFaces(for: style.font))
        \(readerCSS)
        :root { --reader-font: \(style.font.cssFamily); --reader-size: \(Int(size))px; --reader-measure: \(style.width.maxCh)ch;\(antiope ? teletextVariables : "") }
        </style>
        </head>
        <body>
        <article>
        \(antiope ? #"<p class="subpage" id="subpage">\#(escapeText(subpage?.text ?? ""))</p>"# : "")
        <p class="meta">\(meta.joined(separator: #"<span class="dot">·</span>"#))</p>
        <h1 class="title">\(antiope ? doubleHeight(text: title, heading: heading) : heading)</h1>
```

**`apps/macos/Ressac/Reading/ArticleTemplate.swift`, edit 3.** Replace:

```swift

    // MARK: Resources
```

with:

```swift

    /// `:root`'s attributes: the light or dark scheme, plus `data-theme="antiope"` and, with the
    /// CRT screen on, `data-crt` (spec §7 and §9).
    private static func rootAttributes(colorScheme: ColorScheme, theme: ReaderTheme) -> String {
        var attributes = #"lang="en" data-scheme="\#(colorScheme == .dark ? "dark" : "light")""#
        if theme.appearance == .antiope { attributes += #" data-theme="antiope""# }
        if theme.showsCRT { attributes += " data-crt" }
        return attributes
    }

    /// The Antiope title over two rows (spec §7): a hidden copy at twice the row height gives the
    /// layout its height, whatever the number of lines, and the visible copy is stretched over
    /// it with `scaleY(2)`. Only the visible copy carries the link and reaches VoiceOver.
    private static func doubleHeight(text: String, heading: String) -> String {
        #"<span class="dh-space" aria-hidden="true">\#(text)</span><span class="dh-ink">\#(heading)</span>"#
    }

    /// Antiope's extra custom properties: the teletext font for every role of `reader.css`.
    private static let teletextVariables = #" --teletext-font: "\#(TeletextFont.familyName)", monospace;"#

    // MARK: Resources
```

**`apps/macos/Ressac/Reading/ArticleTemplate.swift`, edit 4.** Replace:

```swift
    private static let atkinsonFaces = makeFontFaces(for: .atkinson)
```

with:

```swift
    private static let atkinsonFaces = makeFontFaces(for: .atkinson)

    /// Bedstead embedded the same way (spec §7). Empty when the file is missing, but Antiope is
    /// never the effective appearance then (`Appearance.effective`).
    private static let teletextFace: String = {
        guard let url = Bundle.main.url(forResource: TeletextFont.fileName, withExtension: "otf", subdirectory: "Fonts"),
              let data = try? Data(contentsOf: url)
        else { return "" }
        return """
        @font-face { font-family: "\(TeletextFont.familyName)"; font-style: normal; font-weight: 400; \
        src: url("data:font/otf;base64,\(data.base64EncodedString())") format("opentype"); }
        """
    }()
```

- [ ] **Step 4: Implement the stylesheet.** Append the two sections to the end of `reader.css` (they must come after the `data-scheme` palettes: same specificity, so the later rule wins):

```css
/*
 * Antiope begin. The teletext look (spec §7): Bedstead everywhere, one size, the eight
 * teletext colours on black, every vertical distance a whole number of rows. It ignores
 * data-scheme. ArticleTemplate sets data-theme="antiope" and --teletext-font on :root.
 * Only the eight colours appear in this section (a test checks it).
 */

:root[data-theme="antiope"] {
  color-scheme: dark;
  --tt-black: #000000;
  --tt-red: #FF0000;
  --tt-green: #00FF00;
  --tt-yellow: #FFFF00;
  --tt-blue: #0000FF;
  --tt-magenta: #FF00FF;
  --tt-cyan: #00FFFF;
  --tt-white: #FFFFFF;
  --row: 1.2em;
  --cell: 0.6em;
  --ui-font: var(--teletext-font);
  --mono-font: var(--teletext-font);
  --reader-font: var(--teletext-font);
  --paper: var(--tt-black);
  --ink: var(--tt-white);
  --ink-secondary: var(--tt-cyan);
  --ink-tertiary: var(--tt-cyan);
  --link: var(--tt-cyan);
  --link-underline: var(--tt-cyan);
  --rule: var(--tt-blue);
  --code-bg: var(--tt-black);
  --quote-bar: var(--tt-magenta);
  --mark: var(--tt-yellow);
  --badge-bg: var(--tt-green);
  --notice-bg: var(--tt-red);
  --notice-ink: var(--tt-black);
  --selection: var(--tt-cyan);
  --image-filter: none;
  --leading: var(--row);
  --smoothing: none;
}

:root[data-theme="antiope"] body {
  padding: var(--row) calc(2 * var(--cell)) calc(4 * var(--row));
  font-kerning: none;
  font-variant-ligatures: none;
  font-synthesis: none;
  text-rendering: optimizeSpeed;
  hanging-punctuation: none;
  letter-spacing: 0;
}

:root[data-theme="antiope"] ::selection {
  background: var(--tt-cyan);
  color: var(--tt-black);
}

/* One size, one weight, one row: teletext has no other. */
:root[data-theme="antiope"] :is(h1, h2, h3, h4, h5, h6, .meta, .badge, .byline, .notice, code, kbd, samp, pre, small, sub, sup, table) {
  font-size: 1em;
  font-weight: normal;
  font-style: normal;
  letter-spacing: 0;
  line-height: var(--row);
}

:root[data-theme="antiope"] :is(sub, sup) {
  vertical-align: baseline;
}

:root[data-theme="antiope"] :is(p, ul, ol, dl, pre, blockquote, table, figure) {
  margin: 0 0 var(--row);
}

:root[data-theme="antiope"] :is(h2, h3, h4, h5, h6, .content h1) {
  margin: var(--row) 0 0;
}

:root[data-theme="antiope"] h2,
:root[data-theme="antiope"] .content h1 {
  color: var(--tt-green);
}

:root[data-theme="antiope"] h3 {
  color: var(--tt-cyan);
}

/* Header: the sub-page line (412 3/20), the meta line, the double-height title. */

:root[data-theme="antiope"] .subpage {
  min-height: var(--row);
  margin: 0;
  color: var(--tt-yellow);
}

:root[data-theme="antiope"] .meta {
  gap: 0 var(--cell);
  margin: 0;
}

:root[data-theme="antiope"] .meta .badge {
  padding: 0 var(--cell);
  border-radius: 0;
  color: var(--tt-black);
}

/*
 * Double height: the hidden copy, at twice the row height, gives the title its height in
 * whole rows whatever the number of lines; the visible copy, at one row, is stretched over it.
 */
:root[data-theme="antiope"] h1.title {
  display: grid;
  margin: var(--row) 0;
  color: var(--tt-yellow);
}

:root[data-theme="antiope"] h1.title > span {
  grid-area: 1 / 1;
}

:root[data-theme="antiope"] h1.title .dh-space {
  visibility: hidden;
  line-height: calc(2 * var(--row));
}

:root[data-theme="antiope"] h1.title .dh-ink {
  transform: scaleY(2);
  transform-origin: 0 0;
}

:root[data-theme="antiope"] .byline {
  margin: 0;
}

:root[data-theme="antiope"] .notice {
  margin: var(--row) 0 0;
  padding: 0 var(--cell);
  border-radius: 0;
}

:root[data-theme="antiope"] .content {
  margin-top: var(--row);
}

:root[data-theme="antiope"] .empty {
  font-style: normal;
}

/* Body. */

:root[data-theme="antiope"] a,
:root[data-theme="antiope"] a:hover {
  text-decoration: none;
}

:root[data-theme="antiope"] :is(ul, ol) {
  padding-left: calc(2 * var(--cell));
}

:root[data-theme="antiope"] li + li {
  margin-top: 0;
}

/* A magenta mosaic bar: the left sub-pixel column of each cell, then one cell of space. */
:root[data-theme="antiope"] blockquote {
  padding-left: calc(1.5 * var(--cell));
  border-left: calc(0.5 * var(--cell)) solid var(--quote-bar);
  color: var(--ink);
}

:root[data-theme="antiope"] :is(code, pre) {
  padding: 0;
  border-radius: 0;
  color: var(--tt-green);
}

:root[data-theme="antiope"] mark {
  color: var(--tt-black);
}

/* One font pixel of blue, centred in one row. */
:root[data-theme="antiope"] hr {
  margin: calc(0.5 * var(--row) - 0.05em) 0;
  border-top-width: 0.1em;
}

:root[data-theme="antiope"] :is(th, td) {
  padding: 0 var(--cell);
  border-bottom-width: 0.1em;
}

:root[data-theme="antiope"] th {
  color: var(--tt-yellow);
}

:root[data-theme="antiope"] img {
  border-radius: 0;
}

/* Antiope end. */

/*
 * CRT begin. The screen's colour bleed (spec §9): ArticleTemplate adds data-crt in Antiope when
 * the CRT screen box is checked. Text only: images and mosaics get none.
 */
:root[data-theme="antiope"][data-crt] body {
  text-shadow: 1px 0 rgba(255, 0, 0, 0.45), -1px 0 rgba(0, 0, 255, 0.45), 0 0 2px currentColor;
}

/* CRT end. */
```

- [ ] **Step 5: Run green.** Same command as Step 2. Expected: `AntiopePageTests` (3 tests) and `AntiopeStylesheetTests` (3 tests) pass. If only the stylesheet tests fail, `reader.css` in `Ressac.app/Contents/Resources/` is stale: run `xcodegen generate` again. If only `bedsteadIsEmbeddedInAntiopeOnly` fails, `bedstead.otf` is not in `Ressac.app/Contents/Resources/Fonts/` (phase 3).

- [ ] **Step 6: Run the whole reader suite.** `… test -only-testing:RessacTests/ArticlePageTests -only-testing:RessacTests/SanitizerTests -only-testing:RessacTests/FirstRunTests`. Expected: all pass unchanged (the Native page is the same).

- [ ] **Step 7: Commit.**

```bash
git add apps/macos/Ressac/Reading/ArticleTemplate.swift apps/macos/Ressac/Resources/reader.css apps/macos/RessacTests/ReaderTests.swift
git commit -m "Reader: Antiope page and stylesheet, Bedstead embedded, double-height title, CRT bleed

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 10b.3: The reader view passes the theme and keeps the sub-page line current

**Files:**
- Modify: `apps/macos/Ressac/Reading/ReaderView.swift`
- Test: `apps/macos/RessacTests/ReaderTests.swift`

**Interfaces:**
- Consumes: `Appearance.Keys.appearance`, `Appearance.Keys.crt`, `Appearance.effective(stored:fontAvailable:)`; `ReaderStyle.Keys.antiopeSize`, `TeletextFont.isAvailable`, `TeletextFont.defaultSize`; `PageDirectory(snapshot:)`, `page(forFeed:)`; `ReaderTheme`, `ReaderStyle.effectiveSize`; `SubpageLabel`; `ArticleTemplate.page(…theme:subpage:)`.
- Produces: `ArticlePage.theme` and `ArticlePage.subpage` (defaulted `var`s, so `ArticlePage(entry:fulltext:showFull:style:colorScheme:)` still compiles); `ReaderController.showSubpage(_:) async`.

**What changes in `ReaderView.swift`:**
- `ReaderView` reads `@AppStorage(Appearance.Keys.appearance)` and `@AppStorage(Appearance.Keys.crt)`. The theme is Antiope only when Bedstead is available (`Appearance.effective`, so Antiope never draws in a fallback font); `crt` is the box as stored. Phase 13 adds Increase contrast to the same `ReaderTheme(` call.
- The page's size is `ReaderStyle.effectiveSize` of the look's own stored size (`reader.size` in Native, `reader.size.antiope` in Antiope); the sub-page label is computed in Antiope only (Native pages never change with the list).
- `ArticlePage` equality includes the theme and **excludes the sub-page label**: a refresh that grows the list must not reload the page. When only the label changed, `ArticleWebView.updateNSView` rewrites the line through app JavaScript (`callAsyncJavaScript` in the `.page` world, which runs with page JavaScript off: notes, 11/10) if that entry's document is committed, and reloads otherwise (nothing to keep yet).
- The under-page colour (seen past the page edges when scrolling) is black in Antiope.

- [ ] **Step 1: Write the failing tests.** Append to `ReaderTests.swift` (`eventually` is the helper of `RunCoordinatorTests.swift`):

```swift
@Suite struct AntiopeReaderViewTests {
    @Test func pageIdentityIgnoresTheSubpageButNotTheTheme() {
        let a = ArticlePage(entry: entry(), fulltext: nil, showFull: false, style: ReaderStyle(), colorScheme: .dark,
                            theme: ReaderTheme(appearance: .antiope, crt: false), subpage: SubpageLabel(page: 412, index: 1, count: 2))
        var b = a
        b.subpage = SubpageLabel(page: 412, index: 1, count: 3)
        var c = a
        c.theme = ReaderTheme(appearance: .antiope, crt: true)
        #expect(a == b)
        #expect(a != c)
    }
}

@MainActor @Suite struct SubpageLineTests {
    @Test func theLineChangesInPlaceWithoutAReload() async throws {
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 600, height: 400), styleMask: [.titled], backing: .buffered, defer: false)
        let webView = WKWebView(frame: window.contentLayoutRect)
        window.contentView = webView
        let controller = ReaderController()
        controller.webView = webView
        let coordinator = ArticleWebView.Coordinator(controller: controller)
        webView.navigationDelegate = coordinator
        let html = ArticleTemplate.page(for: entry(content: String(repeating: "<p>Line of a long article.</p>", count: 200)),
                                        fulltext: nil, showFull: false, style: ReaderStyle(), colorScheme: .dark,
                                        theme: ReaderTheme(appearance: .antiope, crt: false), subpage: SubpageLabel(page: 412, index: 3, count: 20))
        controller.load(entryId: 1, html: html)
        #expect(await eventually { controller.loadedEntryId == 1 })
        _ = try await webView.callAsyncJavaScript("window.scrollTo(0, 300)", contentWorld: .page)
        await controller.showSubpage(SubpageLabel(page: 412, index: 3, count: 21))
        let line = try await webView.callAsyncJavaScript(#"return document.getElementById("subpage").textContent"#, contentWorld: .page)
        #expect(line as? String == "412 3/21")
        let scroll = try await webView.callAsyncJavaScript("return window.scrollY", contentWorld: .page)
        #expect((scroll as? Double ?? 0) > 0)  // same document: the position is kept
    }
}
```

- [ ] **Step 2: Run red.** `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/AntiopeReaderViewTests -only-testing:RessacTests/SubpageLineTests`. Expected: build failure, `extra arguments at positions #6, #7 in call` on `ArticlePage` and `value of type 'ReaderController' has no member 'showSubpage'`.

- [ ] **Step 3: Implement.** Re-read `ReaderView.swift` first: phases 8 and 9 may have touched it (they were written in parallel with this plan). Apply the edits in order; if a **Replace** block no longer matches because of such a change, merge by hand and keep their change. The file ends at about 300 lines.

**`apps/macos/Ressac/Reading/ReaderView.swift`, edit 1.** Replace:

```swift
    @AppStorage(ReaderStyle.Keys.width) private var width = LineWidth.medium
```

with:

```swift
    @AppStorage(ReaderStyle.Keys.width) private var width = LineWidth.medium
    @AppStorage(ReaderStyle.Keys.antiopeSize) private var antiopeSize = TeletextFont.defaultSize
    @AppStorage(Appearance.Keys.appearance) private var storedAppearance = Appearance.native
    @AppStorage(Appearance.Keys.crt) private var crt = false
```

**`apps/macos/Ressac/Reading/ReaderView.swift`, edit 2.** Replace:

```swift
    private func page(for entry: EntryRow) -> ArticlePage {
        ArticlePage(entry: entry, fulltext: state.selectedFulltext, showFull: state.readerShowsFullText,
                    style: ReaderStyle(font: font, size: ReaderStyle.clampedSize(size), width: width),
                    colorScheme: colorScheme)
```

with:

```swift
    private func page(for entry: EntryRow) -> ArticlePage {
        let theme = theme
        return ArticlePage(entry: entry, fulltext: state.selectedFulltext, showFull: state.readerShowsFullText,
                           style: ReaderStyle(font: font, size: ReaderStyle.effectiveSize(theme.appearance == .antiope ? antiopeSize : size,
                                                                                         appearance: theme.appearance), width: width),
                           colorScheme: colorScheme, theme: theme, subpage: subpage(for: entry, theme: theme))
    }

    /// Antiope only when Bedstead is there (spec §5). `crt` is the CRT screen box; phase 13 also
    /// turns it off under Increase contrast, in this call (spec §9).
    private var theme: ReaderTheme {
        ReaderTheme(appearance: Appearance.effective(stored: storedAppearance, fontAvailable: TeletextFont.isAvailable),
                    crt: crt)
    }

    /// "412 3/20" in Antiope (spec §6); nil in Native, so the native page never changes with it.
    private func subpage(for entry: EntryRow, theme: ReaderTheme) -> SubpageLabel? {
        guard theme.appearance == .antiope else { return nil }
        let directory = PageDirectory(snapshot: state.sidebar)
        return SubpageLabel.of(entryId: entry.entryId, in: state.entries, page: directory.page(forFeed:))
```

**`apps/macos/Ressac/Reading/ReaderView.swift`, edit 3.** Replace:

```swift
/// Everything the page depends on. Read and starred flags are left out on purpose: marking the
/// entry read must not reload the page and lose the scroll position.
struct ArticlePage: Equatable {
    let entry: EntryRow
    let fulltext: FulltextRow?
    let showFull: Bool
    let style: ReaderStyle
    let colorScheme: ColorScheme

    static func == (a: ArticlePage, b: ArticlePage) -> Bool {
        a.entry.entryId == b.entry.entryId && a.entry.title == b.entry.title && a.entry.url == b.entry.url
            && a.entry.author == b.entry.author && a.entry.summaryHtml == b.entry.summaryHtml
            && a.entry.contentHtml == b.entry.contentHtml && a.fulltext == b.fulltext && a.showFull == b.showFull
            && a.style == b.style && a.colorScheme == b.colorScheme
    }

    func html() -> String {
        ArticleTemplate.page(for: entry, fulltext: fulltext, showFull: showFull, style: style, colorScheme: colorScheme)
```

with:

```swift
/// Everything the page depends on. Read and starred flags are left out on purpose: marking the
/// entry read must not reload the page and lose the scroll position. The sub-page line is left
/// out too: the list grows during a refresh, and `ArticleWebView` rewrites that line in place.
struct ArticlePage: Equatable {
    let entry: EntryRow
    let fulltext: FulltextRow?
    let showFull: Bool
    let style: ReaderStyle
    let colorScheme: ColorScheme
    var theme: ReaderTheme = .native
    var subpage: SubpageLabel?

    static func == (a: ArticlePage, b: ArticlePage) -> Bool {
        a.entry.entryId == b.entry.entryId && a.entry.title == b.entry.title && a.entry.url == b.entry.url
            && a.entry.author == b.entry.author && a.entry.summaryHtml == b.entry.summaryHtml
            && a.entry.contentHtml == b.entry.contentHtml && a.fulltext == b.fulltext && a.showFull == b.showFull
            && a.style == b.style && a.colorScheme == b.colorScheme && a.theme == b.theme
    }

    func html() -> String {
        ArticleTemplate.page(for: entry, fulltext: fulltext, showFull: showFull, style: style, colorScheme: colorScheme,
                             theme: theme, subpage: subpage)
```

**`apps/macos/Ressac/Reading/ReaderView.swift`, edit 4.** Replace:

```swift
        return (result as? Bool) ?? false
    }
}
```

with:

```swift
        return (result as? Bool) ?? false
    }

    /// Rewrites the Antiope sub-page line of the page on screen, keeping its scroll position.
    func showSubpage(_ label: SubpageLabel?) async {
        guard let webView else { return }
        let script = #"const line = document.getElementById("subpage"); if (line) { line.textContent = text; }"#
        _ = try? await webView.callAsyncJavaScript(script, arguments: ["text": label?.text ?? ""], contentWorld: .page)
    }
}
```

**`apps/macos/Ressac/Reading/ReaderView.swift`, edit 5.** Replace:

```swift
        webView.allowsMagnification = false
        webView.underPageBackgroundColor = NSColor(name: nil) { appearance in
            appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
                ? NSColor(srgbRed: 0x1C / 255, green: 0x1C / 255, blue: 0x1E / 255, alpha: 1)
                : .white
        }
        controller.webView = webView
        return webView
    }

    func updateNSView(_ webView: WKWebView, context: Context) {
        guard context.coordinator.page != page else { return }
        context.coordinator.page = page
        controller.load(entryId: page.entry.entryId, html: page.html())
```

with:

```swift
        webView.allowsMagnification = false
        controller.webView = webView
        return webView
    }

    func updateNSView(_ webView: WKWebView, context: Context) {
        webView.underPageBackgroundColor = page.theme.appearance == .antiope ? .black : Self.nativeUnderPage
        guard context.coordinator.page != page else {
            // Same document: only the sub-page line may have changed (the list moved under it).
            guard context.coordinator.page?.subpage != page.subpage else { return }
            context.coordinator.page = page
            if controller.loadedEntryId == page.entry.entryId {
                Task { await controller.showSubpage(page.subpage) }
            } else {
                controller.load(entryId: page.entry.entryId, html: page.html())  // still loading: nothing to keep
            }
            return
        }
        context.coordinator.page = page
        controller.load(entryId: page.entry.entryId, html: page.html())
    }

    /// The colour past the page's edges in Native; Antiope's is black (spec §7).
    private static let nativeUnderPage = NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            ? NSColor(srgbRed: 0x1C / 255, green: 0x1C / 255, blue: 0x1E / 255, alpha: 1)
            : .white
```

- [ ] **Step 4: Run green.** Same command. Expected: `AntiopeReaderViewTests` (1 test) and `SubpageLineTests` (1 test) pass. Then `-only-testing:RessacTests/ReaderViewTests -only-testing:RessacTests/ReaderControllerTests -only-testing:RessacTests/ReaderSlowImageTests -only-testing:RessacTests/FullArticleButtonTests`: all pass unchanged.

- [ ] **Step 5: Verification gate (phase 10b).**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test
```

Expected: every command succeeds, `** TEST SUCCEEDED **`, zero warnings.

- [ ] **Step 6: Commit and push.**

```bash
git add apps/macos/Ressac/Reading/ReaderView.swift apps/macos/RessacTests/ReaderTests.swift
git commit -m "Reader view: Antiope theme and CRT flag, sub-page line updated in place

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
git -c credential.helper= -c credential.helper='!gh auth git-credential' push
```

**Stop here for Clément's approval before phase 11a.**

## Notes for the executor

- **Edit blocks** are applied in the order given, top to bottom. Project rule 9: re-read the file after every three edits to it, and before the first.
- **Line counts after this phase:** `ArticleTemplate.swift` about 490 lines, `ReaderView.swift` about 300, `reader.css` about 480. Read files over 500 lines in chunks (project rule 7).
- **No simulated keystrokes.** Everything here is tested in process. Sizes 15 to 30 and the 40-column width on the real database are manual check 7 of spec §11, done in phase 14.
- **1× displays.** At 15 and 25 pt a font pixel is 1.5 and 2.5 CSS pixels: whole device pixels on Retina only. On a non-Retina external screen those two sizes show uneven pixel widths. This is inherent to the spec's size steps, not a bug of this phase.
- **`scaleY(2)` fallback.** If a later WebKit blurs the stretched title, the spec's fallback (§13) is a 2 em font size in the same row grid. It is twice as wide as well as twice as tall, unlike teletext's double height, so record the blur in `notes.md` and ask Clément before switching.
- **`PageDirectory` per evaluation.** `ReaderView` builds a `PageDirectory` from `state.sidebar` each time its body runs in Antiope (a few hundred rows, microseconds). If phase 7 or 8 keeps a directory on `AntiopeState`, use that instead; the label's `page:` closure takes any `(Int64) -> Int?`.
- **The toolbar.** `ReaderView` still declares its native toolbar items; in Antiope the window has no toolbar (spec §5), and `AntiopeRootView` (phase 8) decides how the reader column is framed. Nothing here assumes either way.
- **Colour scheme.** Antiope ignores `data-scheme`, but a light/dark switch still reloads the page (the scheme is part of `ArticlePage`). Harmless; not worth a special case.
