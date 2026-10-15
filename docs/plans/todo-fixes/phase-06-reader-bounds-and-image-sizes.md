# Phase 06: List pages in both looks, bounded sanitiser, image sizes

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Runs after:** Phase 05 (after Phases 01 to 04), which provides `AppState.entryTotal`, `AppState.entryRowAppeared(_:)` and `AppState.listPageSize`. Line numbers below are those of `855cd2c`; no earlier phase of this plan touches these five files, but match on the quoted text anyway. Phase 07 later edits `BannerView` in `MainView.swift`, which is far from the line this phase changes, and cites `ReaderView.swift:388`, one line lower than at `855cd2c` because of Task 6.1. Phase 08 does not touch these files.

**Goal:** finish TODO « Reporté ▸ Lecture » (all but the NSTableView warning, which is excluded):
- **Item 1, views:** both looks load the next page of a long list as its last rows appear, and Antiope's sub-page line counts the whole list ("412 3/5907", not "3/500").
- **Item 3:** "about 600 KB of flat but invalid HTML takes 2–3 s to sanitise, and abandoned sanitising threads can overlap". The guards now follow libxml, so no input reaches its quadratic paths, and parse workers run one at a time.
- **Item 2:** "the 256 KB HTML cap shows very large articles as plain text". Once item 3's guards are in, the cap only bounds linear work, and it goes up to 512 KB.
- **Item 4:** "an image with no dimensions at the end of an article can make Space move on too early". The sanitiser writes `width` and `height` when the feed gives them in another form, and in Antiope the mosaic keeps its height.

**Tech stack:** Swift 6.4, SwiftUI, WebKit, Foundation's `XMLDocument` (`.documentTidyHTML`, that is libxml2's HTML parser), Swift Testing.

## Findings (measured 15/10/2026, release build, on a copy of the real database)

**Item 3: the guards did not match libxml.** `estimatedNestingDepth` pops a close tag down to its match. libxml does not always do that: a misplaced end tag closes the elements above its match only when none of them has a higher end priority (`htmlAutoCloseOnClose`, `htmlEndPriority`: `div` 150, `td`/`th` 160, `tr` 170, `thead`/`tbody`/`tfoot` 180, `table` 190, `head`/`body` 200, `html` 220, the rest 100). So `</b>` never closes an open `div`, and `<b><div>x</b>` repeated nests two levels deeper each time. The estimate said depth 2 ("flat"), libxml built a tree thousands deep, and its time grew with the square of the depth:

| Input | Size | Estimate before | libxml depth | Sanitise |
|---|---|---|---|---|
| `<b><div>x</b>` × 5,000 | 63 KB | 2 | 10,000 | 0.8 s |
| `<b><div>x</b>` × 10,000 | 126 KB | 2 | 20,000 | 4.6 s |
| `<b><div>x</b>` × 20,000 | 253 KB, under the cap | 2 | 40,000 | **23 s** |
| one tag with 20,000 attributes | 164 KB | 1 | 1 | 0.7 s |
| one tag with 60,000 attributes | 516 KB | 1 | 1 | 5.4 s |

The second shape comes from libxml checking a tag's attributes pairwise for duplicates. The 1 s budget hides both from the main thread, which gets the plain-text fallback, but the worker cannot be stopped: it runs on for seconds, and pressing `j` through such entries starts one more worker each time, all running at once.

Everything else tried stays linear: unclosed `<a>`, `<li>`, `<p>`, `<td>` outside a table, nested `<form>`, `<select><option>`, `<i><p>x</i>`, `<p>x<b>y</p>`, `<a>x<p>y</a>`, 100,000 entities. With the priority rule, the estimate is never below libxml's real depth by more than 5 levels, measured over 1,500 random tag soups of up to 3,000 tags. The gap is elements libxml adds on its own, such as an implied `p`.

**Item 2: the cap.** The database holds 3 entries over 256 KB (the largest is 358 KB), 6 over 128 KB and 60 over 64 KB; there is no stored full text yet. The two largest parse in 19 ms (349 KB) and 43 ms (323 KB): real articles cost about 0.1 ms per KB. The densest linear markup measured costs 0.53 ms/KB for tag soup and 0.84 ms/KB for entities. With item 3's guards the cap no longer stands between libxml and a quadratic input, only between it and linear work, so **512 KB** keeps the worst case under half the 1 s budget (about 0.45 s) and real articles near 50 ms. At 256 KB the three largest articles would stay plain text for nothing.

**Item 4: image sizes.** The database holds 8,125 `<img>`:
- 4,015 carry both `width` and `height`. The sanitiser keeps them in Native, and WebKit reserves the box from them before the image loads. But in Antiope **every** mosaic drops them, so every mosaic is 0 px high until its PNG arrives.
- 4,110 carry neither or only one. Of these, 250 give the size in `style` (`width: 600px; height: 400px`), 134 in Blogger's `data-original-width`/`-height` and 60 in WordPress's `data-orig-size="1200,800"`. 56 have only a `srcset` (widths, no height). 3,610 give nothing at all (Oglaf, ActuaLitté, Pluralistic…).
- 579 of the 1,820 entries that end on an image (under 200 characters of text after it) end on one without `width` and `height`.

**What this phase does not change.** `ReaderController.pageDown` still measures the page as it is. Waiting for pending images there would let a hanging image block Space, which the acceptance fixes of sub-project 2a ruled out ("a slow image or a hanging tracking pixel must not keep Space dead"). Images whose feed gives no size at all still have no box until they load. Sanitising stays synchronous in `updateNSView`; with the guards its worst case is about 0.45 s, and real articles take tens of milliseconds.

## Global constraints

- At most 5 files: `ReaderView.swift`, `MainView.swift`, `PageColumns.swift`, `ArticleTemplate.swift`, `ReaderTests.swift`.
- Both looks keep working: every sanitiser change is tested with `mosaics: true` (Antiope) and without (Native), and `ImageBoxTests` lays out both pages in WebKit.
- The sanitiser still emits only what it writes itself: the only new attributes are `width`/`height` (integers) on Native images and `style="min-height: Npx"` (an integer) on mosaic images. The page's CSP already allows inline styles (`style-src 'unsafe-inline'`).
- Tests use `TestDatabase` and loopback addresses (`http://127.0.0.1:9`, a closed port), never the real database or the network. No simulated keystrokes.
- English everywhere. Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review focus

1. **`<b><div>x</b>` repeated, any size**: plain text at once, with no worker left running (`inlineEndTagsThatCannotCloseABlockCountAsDepth`).
2. **A legitimate article that relies on libxml closing tags**, such as `</div>` closing a `b`, `</i>` closing a `p` or unclosed list items: still parsed, not plain text (`theDepthEstimateFollowsLibxml`, `optionalEndTagsStayOnTheParsedPath`).
3. **Pressing `j` quickly through several heavy entries**: one parse at a time, and those whose reader has moved on are skipped (`abandonedWorkersNeverOverlap`).
4. **An Antiope article ending on an image with a size**: the mosaic box is as tall as its rows before the PNG arrives, so Space scrolls to it (`anImageOfKnownSizeHoldsItsPlace`, Antiope pass).
5. **Scrolling a long list with the mouse, in both looks**: the next page loads before the end shows, one page at a time (`EntryListPagingTests`).

---

### Task 6.0: Step 0, dead-code audit of `ArticleTemplate.swift`

`ArticleTemplate.swift` has 543 lines and this phase restructures its guards and its worker. `ReaderView.swift` (394 lines) and `ReaderTests.swift` (531 lines) only get small edits and additions.

- [ ] **Step 1: Audit**

```bash
cd /Users/willow/Sites/ressac
f=apps/macos/Ressac/Reading/ArticleTemplate.swift
wc -l $f
grep -nE 'print\(|debugPrint|NSLog|dump\(|TODO|FIXME' $f
for name in $(grep -oE '(func|var|let) [a-zA-Z_][a-zA-Z0-9_]*' $f | awk '{print $2}' | sort -u); do
  uses=$(grep -rw "$name" apps/macos/Ressac apps/macos/RessacTests | grep -vc "\(func\|var\|let\) $name\b")
  [ "$uses" -eq 0 ] && echo "unused: $name"
done
```

Expected: `543 apps/macos/Ressac/Reading/ArticleTemplate.swift` and nothing else. Its four imports are used: `Synchronization` (`Mutex`), `SwiftUI` (`ColorScheme`), `WebKit` (`WKNavigationType`) and `Foundation`. No Step 0 commit. If something is printed, remove it in its own commit before Task 6.1.

---

### Task 6.1: The sub-page line counts the whole list

**Files:**
- Modify: `apps/macos/Ressac/Reading/ReaderView.swift:63`, `:153-157`
- Test: `apps/macos/RessacTests/ReaderTests.swift:401`

**Interfaces:**
- Consumes: `AppState.entryTotal` (Phase 05).
- Produces: `SubpageLabel.of(entryId:in:total: Int? = nil, page:)`. `total` nil keeps today's count, `entries.count`.

- [ ] **Step 1: Write the failing test**

In `apps/macos/RessacTests/ReaderTests.swift`, replace line 401:

```swift
        #expect(SubpageLabel.of(entryId: 9, in: list) { _ in 412 } == nil)
```

with:

```swift
        #expect(SubpageLabel.of(entryId: 9, in: list) { _ in 412 } == nil)
        // A long list loads its first rows only: the count is the whole list's.
        #expect(SubpageLabel.of(entryId: 6, in: list, total: 5_907) { _ in 412 } == SubpageLabel(page: 412, index: 2, count: 5_907))
```

- [ ] **Step 2: Run it and watch it fail**

```bash
cd /Users/willow/Sites/ressac/apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/SubpageLabelTests
```

Expected: the test target does not build: `extra arguments at positions #3, #4 in call` and `missing argument for parameter 'page' in call` (Swift reads the trailing closure as `total`).

- [ ] **Step 3: Implement**

In `apps/macos/Ressac/Reading/ReaderView.swift`, replace line 63:

```swift
        return SubpageLabel.of(entryId: entry.entryId, in: state.entries, page: directory.page(forFeed:))
```

with:

```swift
        return SubpageLabel.of(entryId: entry.entryId, in: state.entries, total: state.entryTotal, page: directory.page(forFeed:))
```

Replace lines 153-157:

```swift
    /// Numbered from 1 in `entries`; nil when the entry is not in the list (filtered out).
    static func of(entryId: Int64, in entries: [EntryRow], page: (Int64) -> Int?) -> SubpageLabel? {
        guard let position = entries.firstIndex(where: { $0.entryId == entryId }) else { return nil }
        return SubpageLabel(page: page(entries[position].feedId), index: position + 1, count: entries.count)
    }
```

with:

```swift
    /// Numbered from 1 in `entries`, out of `total`: the whole list's size when only its first
    /// rows are loaded, `entries.count` when nil. Nil when the entry is not in the list (filtered out).
    static func of(entryId: Int64, in entries: [EntryRow], total: Int? = nil, page: (Int64) -> Int?) -> SubpageLabel? {
        guard let position = entries.firstIndex(where: { $0.entryId == entryId }) else { return nil }
        return SubpageLabel(page: page(entries[position].feedId), index: position + 1, count: total ?? entries.count)
    }
```

`entryTotal` is observed, so a refresh that grows the list rewrites the line in place: `ArticlePage` equality ignores the sub-page line, and `ArticleWebView` calls `showSubpage` (unchanged).

- [ ] **Step 4: Run it**

Same command as Step 2. Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cd /Users/willow/Sites/ressac
git add apps/macos/Ressac/Reading/ReaderView.swift apps/macos/RessacTests/ReaderTests.swift
git commit -m "$(cat <<'EOF'
Antiope reader: the sub-page line counts the whole list, not the loaded page

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 6.2: Both looks load the next page as their last rows appear

**Files:**
- Modify: `apps/macos/Ressac/Reading/MainView.swift:147-148`
- Modify: `apps/macos/Ressac/Antiope/PageColumns.swift:93`
- Test: `apps/macos/RessacTests/ReaderTests.swift` (new suite appended at the end, after `:531`)

**Interfaces:**
- Consumes: `AppState.entryRowAppeared(_:)`, `AppState.listPageSize` (Phase 05), `makeState()` (`AppStateTests.swift`) and `eventually` (`RunCoordinatorTests.swift`).

`entryRowAppeared(_:)` does the deferring itself: a row appears while the table lays out, and changing `entries` right there is the reentrant operation that AppKit warns about.

- [ ] **Step 1: Write the failing tests**

Append to `apps/macos/RessacTests/ReaderTests.swift`, after its last line (`:531`, the closing `}` of `SubpageLineTests`):

```swift

/// Both looks load the next page of a long list as its last rows appear.
@MainActor @Suite struct EntryListPagingTests {
    /// Five entries listed two at a time, hosted in `root` (an offscreen window, laid out by hand).
    private func pagesIn(_ root: (AppState) -> some View) async throws -> Bool {
        let (state, test, _) = try makeState()
        defer { withExtendedLifetime(test) {} }  // its directory holds the database
        for day in 1...5 {
            try test.entry(feed: 1, published: String(format: "2026-10-%02dT08:00:00.000Z", day))
        }
        state.listPageSize = 2
        state.selection = .feed(1)
        #expect(state.entries.count == 2)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 600), styleMask: [.titled], backing: .buffered, defer: false)
        window.contentView = NSHostingView(rootView: root(state))
        defer { window.contentView = nil }
        return await eventually {
            window.contentView?.layoutSubtreeIfNeeded()
            return state.entries.count == 5
        }
    }

    @Test func theNativeListLoadsMoreAsItsLastRowsAppear() async throws {
        #expect(try await pagesIn { EntryListView(state: $0) })
    }

    @Test func theAntiopeColumnLoadsMoreAsItsLastRowsAppear() async throws {
        #expect(try await pagesIn { EntryPageColumn().environment($0).environment(AntiopeState(app: $0)) })
    }
}
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cd /Users/willow/Sites/ressac/apps/macos && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/EntryListPagingTests
```

Expected: FAIL in both tests after the 3 s of `eventually` (`Expectation failed: try await pagesIn { … }`): the list stays at 2 rows.

- [ ] **Step 3: Implement**

In `apps/macos/Ressac/Reading/MainView.swift`, replace lines 147-148:

```swift
                EntryRowView(entry: entry)
                    .tag(entry.entryId)
```

with:

```swift
                EntryRowView(entry: entry)
                    .tag(entry.entryId)
                    .onAppear { state.entryRowAppeared(entry.entryId) }
```

In `apps/macos/Ressac/Antiope/PageColumns.swift`, replace line 93:

```swift
                        .id(entry.entryId)
```

with:

```swift
                        .id(entry.entryId)
                        .onAppear { app.entryRowAppeared(entry.entryId) }
```

- [ ] **Step 4: Run them**

Same command as Step 2. Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cd /Users/willow/Sites/ressac
git add apps/macos/Ressac/Reading/MainView.swift apps/macos/Ressac/Antiope/PageColumns.swift apps/macos/RessacTests/ReaderTests.swift
git commit -m "$(cat <<'EOF'
Entry list: both looks load the next page as their last rows appear

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 6.3: The depth estimate follows libxml's end-tag rule

**Files:**
- Modify: `apps/macos/Ressac/Reading/ArticleTemplate.swift:57-61`, `:154-159`, `:185-195`, `:201` (helpers before it)
- Test: `apps/macos/RessacTests/ReaderTests.swift:157-161`, new tests before `:164`

**Interfaces:**
- Produces: `ArticleTemplate.estimatedNestingDepth(_ html: String, stoppingAbove limit: Int = .max) -> Int`. It returns as soon as the depth passes `limit`, which bounds the work done per close tag; the sanitiser passes `maxNestingDepth`.

- [ ] **Step 1: Write the failing tests**

In `apps/macos/RessacTests/ReaderTests.swift`, replace lines 157-161:

```swift
        // 200 KB of the same shape: within the cap, so it returns within the budget (parsed or fallback).
        let medium = String(repeating: "<b><div>x</b>", count: 15_000)
        let mediumStart = Date()
        _ = ArticleTemplate.sanitize(medium)
        #expect(Date().timeIntervalSince(mediumStart) < 1.5)
```

with:

```swift
        // 200 KB of the same shape: the depth estimate sends it to the plain-text path, unparsed.
        let medium = String(repeating: "<b><div>x</b>", count: 15_000)
        let mediumStart = Date()
        _ = ArticleTemplate.sanitize(medium)
        #expect(Date().timeIntervalSince(mediumStart) < 0.2)
```

Then insert before this line (`:164`):

```swift
    @Test func optionalEndTagsStayOnTheParsedPath() {
```

the two new tests:

```swift
    /// libxml ignores `</b>` while a `div` is open, so this nests one level deeper each time and
    /// libxml's time grows with the square of the depth: the estimate follows it, the text is plain.
    @Test func inlineEndTagsThatCannotCloseABlockCountAsDepth() {
        let html = String(repeating: "<b><div>x</b>", count: 5_000)  // 63 KB: 0.8 s in libxml
        #expect(ArticleTemplate.estimatedNestingDepth(html) == 10_000)
        let start = Date()
        let clean = ArticleTemplate.sanitize(html)
        #expect(Date().timeIntervalSince(start) < 0.2)
        #expect(!clean.contains("<div") && clean.contains("x"))
        // Where libxml does close: `</div>` closes a `b`, and `</i>` a `p` (the same priority).
        #expect(ArticleTemplate.estimatedNestingDepth(String(repeating: "<div><b>x</div>", count: 1_000)) == 2)
        #expect(ArticleTemplate.estimatedNestingDepth(String(repeating: "<i><p>x</i>", count: 1_000)) == 2)
        // The scan stops once past the limit.
        #expect(ArticleTemplate.estimatedNestingDepth(html, stoppingAbove: 256) == 257)
    }

    /// The estimate against libxml itself: never below its depth by more than the few elements
    /// libxml adds on its own.
    @Test func theDepthEstimateFollowsLibxml() throws {
        func depth(_ node: XMLNode) -> Int {
            1 + (node.children ?? []).filter { $0.kind == .element }.map(depth).reduce(0, max)
        }
        let shapes = [
            String(repeating: "<b><div>x</b>", count: 300), String(repeating: "<div><b>x</div>", count: 300),
            String(repeating: "<i><p>x</i>", count: 300), String(repeating: #"<a href="https://e.org">x<p>y</a>"#, count: 300),
            String(repeating: "<span><div>x</span>", count: 300), String(repeating: "<td><div>x</td>", count: 300),
            String(repeating: "<table><tr><td>x</b>", count: 100), String(repeating: "<ul><li>x<p>y</ul>", count: 300),
        ]
        for html in shapes {
            let document = try XMLDocument(xmlString: "<html><body>\(html)</body></html>",
                                           options: [.documentTidyHTML, .nodeLoadExternalEntitiesNever])
            let body = try #require(document.rootElement()?.children?.first { $0.name == "body" })
            #expect(ArticleTemplate.estimatedNestingDepth(html) + 5 >= depth(body) - 1, "\(html.prefix(30))")
        }
    }

```

- [ ] **Step 2: Run them and watch them fail**

```bash
cd /Users/willow/Sites/ressac/apps/macos && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/SanitizerTests
```

Expected: the test target does not build: `extra argument 'stoppingAbove' in call`. (Without that line, `theDepthEstimateFollowsLibxml` fails for the `<b><div>x</b>` and `<span><div>x</span>` shapes, and the `medium` timing fails at about 1 s.)

- [ ] **Step 3: Implement (re-read `ArticleTemplate.swift` first)**

Replace lines 57-61:

```swift
        // Tidy and the recursive renderer overflow the stack on pathological nesting, and libxml's
        // repair of misnested end tags is quadratic, so such input (or anything big) skips the
        // parser and is shown as escaped plain text.
        let size = html.utf8.count
        if size > maxParsedBytes || estimatedNestingDepth(html) > maxNestingDepth {
```

with:

```swift
        // Tidy and the recursive renderer overflow the stack on pathological nesting, and libxml's
        // work grows with the square of the depth, so such input (or anything big) skips the
        // parser and is shown as escaped plain text.
        let size = html.utf8.count
        if size > maxParsedBytes || estimatedNestingDepth(html, stoppingAbove: maxNestingDepth) > maxNestingDepth {
```

Replace lines 154-159:

```swift
    /// Linear, non-recursive estimate of the deepest element nesting: opening tags minus closing
    /// tags, ignoring void and self-closing tags, comments, `<!…>` and `<?…>`.
    static func estimatedNestingDepth(_ html: String) -> Int {
        let bytes = Array(html.utf8)
        // Conservative: it may overestimate, never underestimate. A close tag pops only down to a
        // matching open name (a stray one pops nothing), and a tag ends at the first `>`.
```

with:

```swift
    /// Linear, non-recursive estimate of the deepest element nesting: opening tags minus closing
    /// tags, ignoring void and self-closing tags, comments, `<!…>` and `<?…>`. Stops as soon as
    /// the depth passes `limit`, which bounds the work per close tag.
    static func estimatedNestingDepth(_ html: String, stoppingAbove limit: Int = .max) -> Int {
        let bytes = Array(html.utf8)
        // Conservative: it may overestimate, and underestimates only by the few elements libxml
        // adds on its own (an implied `p`: 5 levels at most over 1,500 random tag soups). A close
        // tag pops down to its open name only where libxml would (`closes(_:above:)`), a stray one
        // pops nothing, and a tag ends at the first `>`.
```

Replace lines 185-195:

```swift
            if closing {
                if open[tag, default: 0] > 0 {
                    while let top = stack.last, top != tag { pop() }
                    pop()
                }
            } else if !voidTags.contains(tag) {
                closeOptionalEndTags(before: tag, stack: stack, pop: pop)
                stack.append(tag)
                open[tag, default: 0] += 1
                deepest = max(deepest, stack.count)
            }
```

with:

```swift
            if closing {
                if open[tag, default: 0] > 0, let match = stack.lastIndex(of: tag),
                   closes(tag, above: stack[(match + 1)...]) {
                    while stack.count > match { pop() }
                }
            } else if !voidTags.contains(tag) {
                closeOptionalEndTags(before: tag, stack: stack, pop: pop)
                stack.append(tag)
                open[tag, default: 0] += 1
                deepest = max(deepest, stack.count)
                if deepest > limit { return deepest }
            }
```

Read the three places back. Then insert before line 201:

```swift
    /// HTML's optional end tags: what an opening tag closes when it sits on top of the stack.
```

the two helpers:

```swift
    /// libxml's rule for a misplaced end tag (`htmlAutoCloseOnClose`): it closes the elements
    /// opened after its match only when none of them has a higher end priority than its own;
    /// otherwise it is ignored. So `</b>` never closes an open `div`, and `<b><div>x</b>`,
    /// repeated, nests one level deeper each time (60 KB of it took 0.8 s, 250 KB 23 s).
    private static func closes(_ tag: String, above opened: ArraySlice<String>) -> Bool {
        let priority = endPriority(tag)
        return opened.allSatisfy { endPriority($0) <= priority }
    }

    /// libxml's `htmlEndPriority` table; every other element has 100.
    private static func endPriority(_ tag: String) -> Int {
        switch tag {
        case "div": 150
        case "td", "th": 160
        case "tr": 170
        case "thead", "tbody", "tfoot": 180
        case "table": 190
        case "head", "body": 200
        case "html": 220
        default: 100
        }
    }

```

The new rule pops less often than the old one, so the estimate can only go up: it stays conservative. With `stoppingAbove`, the stack never holds more than `limit + 1` names, so each close tag's scan is bounded and the whole pass stays linear (a stack of 50,000 open `span`s under a `div` would otherwise cost a scan per stray `</b>`).

- [ ] **Step 4: Run the sanitiser tests**

```bash
cd /Users/willow/Sites/ressac/apps/macos && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/SanitizerTests -only-testing:RessacTests/MosaicMarkupTests -only-testing:RessacTests/SanitiseTimeoutTests
```

Expected: PASS, including the existing `deepNestingDoesNotOverflowTheStack` (its 300-level `blockquote` still estimates exactly 300) and `optionalEndTagsStayOnTheParsedPath`.

- [ ] **Step 5: Commit**

```bash
cd /Users/willow/Sites/ressac
git add apps/macos/Ressac/Reading/ArticleTemplate.swift apps/macos/RessacTests/ReaderTests.swift
git commit -m "$(cat <<'EOF'
Sanitiser: the depth estimate follows libxml's end-tag priorities

`</b>` never closes an open `div` in libxml, so `<b><div>x</b>` nested at
every step while the estimate said 2: 250 KB of it took 23 s to parse.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 6.4: A tag with hundreds of attributes is shown as text

**Files:**
- Modify: `apps/macos/Ressac/Reading/ArticleTemplate.swift` (the guard in `sanitizeUncached`, as Task 6.3 left it; `:147`; a new function before `:201`)
- Test: `apps/macos/RessacTests/ReaderTests.swift` (new test before `optionalEndTagsStayOnTheParsedPath`)

**Interfaces:**
- Produces: `ArticleTemplate.maxAttributesPerTag = 256` and `ArticleTemplate.mostAttributesOnATag(_ html: String, stoppingAbove limit: Int = .max) -> Int`. It counts attribute names on each start tag and skips values whole, so a quoted value may hold spaces and `>`. It returns `limit + 1` as soon as one tag passes `limit`.

- [ ] **Step 1: Write the failing test**

In `apps/macos/RessacTests/ReaderTests.swift`, insert before `    @Test func optionalEndTagsStayOnTheParsedPath() {`:

```swift
    /// libxml checks a tag's attributes pairwise: past `maxAttributesPerTag`, plain text.
    @Test func aTagWithThousandsOfAttributesIsShownAsText() {
        let many = "<p " + (0..<20_000).map { "a\($0)=x" }.joined(separator: " ") + ">kept</p>"  // 164 KB: 0.7 s in libxml
        #expect(ArticleTemplate.mostAttributesOnATag(many) == 20_000)
        #expect(ArticleTemplate.mostAttributesOnATag(many, stoppingAbove: 256) == 257)
        let start = Date()
        #expect(ArticleTemplate.sanitize(many) == "<p>kept</p>")
        #expect(Date().timeIntervalSince(start) < 0.2)
        // Quoted values are skipped whole, spaces and `>` included; unquoted ones up to a space.
        #expect(ArticleTemplate.mostAttributesOnATag(#"<p>x</p><img src="a b>c" alt='d e' width=600 data-x = "1">"#) == 4)
        #expect(ArticleTemplate.mostAttributesOnATag("<p>no attributes</p><br/>") == 0)
        let fine = "<p " + (0..<200).map { "a\($0)=x" }.joined(separator: " ") + ">kept</p>"
        #expect(ArticleTemplate.sanitize(fine).hasPrefix("<p>kept"))
    }

```

- [ ] **Step 2: Run it and watch it fail**

```bash
cd /Users/willow/Sites/ressac/apps/macos && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/SanitizerTests
```

Expected: the test target does not build: `type 'ArticleTemplate' has no member 'mostAttributesOnATag'`.

- [ ] **Step 3: Implement (re-read `ArticleTemplate.swift` first)**

Replace, in `sanitizeUncached`, the lines Task 6.3 wrote:

```swift
        // Tidy and the recursive renderer overflow the stack on pathological nesting, and libxml's
        // work grows with the square of the depth, so such input (or anything big) skips the
        // parser and is shown as escaped plain text.
        let size = html.utf8.count
        if size > maxParsedBytes || estimatedNestingDepth(html, stoppingAbove: maxNestingDepth) > maxNestingDepth {
```

with:

```swift
        // Tidy and the recursive renderer overflow the stack on pathological nesting, and libxml's
        // work grows with the square of the depth and of a tag's attributes, so such input (or
        // anything big) skips the parser and is shown as escaped plain text.
        let size = html.utf8.count
        if size > maxParsedBytes || estimatedNestingDepth(html, stoppingAbove: maxNestingDepth) > maxNestingDepth
            || mostAttributesOnATag(html, stoppingAbove: maxAttributesPerTag) > maxAttributesPerTag {
```

Replace line 147:

```swift
    static let maxNestingDepth = 256
```

with:

```swift
    static let maxNestingDepth = 256
    /// libxml checks a tag's attributes pairwise for duplicates: 60,000 on one tag took 5 s.
    static let maxAttributesPerTag = 256
```

Insert before the line:

```swift
    /// HTML's optional end tags: what an opening tag closes when it sits on top of the stack.
```

(that is, right after Task 6.3's `endPriority`) the counter:

```swift
    /// The most attributes on one start tag. A value is skipped whole (quoted, it may hold
    /// spaces and `>`), so only names count; an unquoted value is read up to a space or `>`.
    /// Linear; stops as soon as a tag has more than `limit` (and returns `limit + 1`).
    static func mostAttributesOnATag(_ html: String, stoppingAbove limit: Int = .max) -> Int {
        let bytes = Array(html.utf8)
        func isSpace(_ b: UInt8) -> Bool { b == 0x20 || b == 0x09 || b == 0x0A || b == 0x0C || b == 0x0D }
        var most = 0, i = 0
        while i < bytes.count {
            guard bytes[i] == 0x3C, i + 1 < bytes.count, isASCIILetter(bytes[i + 1]) else { i += 1; continue }  // <a
            var j = i + 1
            while j < bytes.count, !isSpace(bytes[j]), bytes[j] != 0x3E, bytes[j] != 0x2F { j += 1 }  // the tag name
            var count = 0
            scan: while j < bytes.count {
                switch bytes[j] {
                case 0x3E:  // >
                    break scan
                case 0x3D:  // =: skip the value
                    j += 1
                    while j < bytes.count, isSpace(bytes[j]) { j += 1 }
                    if j < bytes.count, bytes[j] == 0x22 || bytes[j] == 0x27 {
                        let quote = bytes[j]
                        j += 1
                        while j < bytes.count, bytes[j] != quote { j += 1 }
                        j += 1
                    } else {
                        while j < bytes.count, !isSpace(bytes[j]), bytes[j] != 0x3E { j += 1 }
                    }
                case let b where isSpace(b) || b == 0x2F:  // spaces and /
                    j += 1
                default:  // a name
                    count += 1
                    if count > limit { return count }
                    while j < bytes.count, !isSpace(bytes[j]), bytes[j] != 0x3E, bytes[j] != 0x3D, bytes[j] != 0x2F { j += 1 }
                }
            }
            most = max(most, count)
            i = j + 1
        }
        return most
    }

```

Read the three places back. A quoted value runs to its closing quote even across `>`, as libxml reads it; an unclosed quote runs to the end of the input, as in libxml. The real feeds' busiest tags (WordPress images) carry about 20 attributes.

- [ ] **Step 4: Run the sanitiser tests**

Same command as Task 6.3, Step 4. Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cd /Users/willow/Sites/ressac
git add apps/macos/Ressac/Reading/ArticleTemplate.swift apps/macos/RessacTests/ReaderTests.swift
git commit -m "$(cat <<'EOF'
Sanitiser: a tag with more than 256 attributes is shown as text

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 6.5: Parse workers run one at a time

**Files:**
- Modify: `apps/macos/Ressac/Reading/ArticleTemplate.swift:125-143`
- Test: `apps/macos/RessacTests/ReaderTests.swift:1-4` (import), new test before `optionalEndTagsStayOnTheParsedPath`

**Interfaces:**
- Produces: `ArticleTemplate.onLargeStack(timeout:_:)` keeps its signature. Workers now pass through one gate: each one waits for the previous worker, and skips its work if its caller stopped waiting meanwhile. A worker that is already running still finishes, and its late result is dropped. At most one parse runs at any time, and only the one already running when its caller gave up is wasted.

With Tasks 6.3 and 6.4, no known input keeps libxml busy past the budget, so this is the backstop for shapes nobody has found yet. The cost: a parse that comes right after an abandoned one waits for it, within its own budget.

- [ ] **Step 1: Write the failing test**

In `apps/macos/RessacTests/ReaderTests.swift`, replace lines 1-4:

```swift
import Foundation
import SwiftUI
import Testing
import WebKit
```

with:

```swift
import Foundation
import SwiftUI
import Synchronization
import Testing
import WebKit
```

Then insert before `    @Test func optionalEndTagsStayOnTheParsedPath() {`:

```swift
    /// One worker at a time: abandoned parses wait their turn, then skip it.
    @Test func abandonedWorkersNeverOverlap() {
        final class Probe: Sendable {
            let running = Mutex(0)
            let peak = Mutex(0)
            let runs = Mutex(0)
        }
        let probe = Probe()
        let work: @Sendable () -> String = {
            let now = probe.running.withLock { $0 += 1; return $0 }
            probe.peak.withLock { $0 = max($0, now) }
            probe.runs.withLock { $0 += 1 }
            Thread.sleep(forTimeInterval: 1)
            probe.running.withLock { $0 -= 1 }
            return "done"
        }
        #expect(ArticleTemplate.onLargeStack(timeout: 0.3, work) == nil)  // abandoned while it runs
        for _ in 0..<3 {
            #expect(ArticleTemplate.onLargeStack(timeout: 0.05, work) == nil)  // abandoned before their turn
        }
        #expect(ArticleTemplate.onLargeStack(timeout: 5, work) == "done")
        #expect(probe.peak.withLock { $0 } == 1)
        #expect(probe.runs.withLock { $0 } <= 2)
    }

```

- [ ] **Step 2: Run it and watch it fail**

```bash
cd /Users/willow/Sites/ressac/apps/macos && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:'RessacTests/SanitizerTests/abandonedWorkersNeverOverlap()'
```

Expected: FAIL on `probe.peak.withLock { $0 } == 1` and `probe.runs.withLock { $0 } <= 2`: today all five workers start at once.

- [ ] **Step 3: Implement (re-read `ArticleTemplate.swift` first)**

Replace lines 125-143:

```swift
    private final class ResultBox: @unchecked Sendable { var value = "" }

    /// Runs `work` on a thread with a 256 MB stack (a virtual reservation) and waits for it, so
    /// libxml's recursive parser cannot exhaust the stack on input the depth guard let through.
    /// Returns `nil` when `timeout` passes first. The worker cannot be cancelled: it finishes in
    /// the background and its late result is discarded; its cost is bounded by `maxParsedBytes`.
    static func onLargeStack(timeout: TimeInterval = .infinity, _ work: @escaping @Sendable () -> String) -> String? {
        let box = ResultBox()
        let done = DispatchSemaphore(value: 0)
        let thread = Thread {
            box.value = work()
            done.signal()
        }
        thread.stackSize = 256 << 20
        thread.qualityOfService = .userInteractive
        thread.start()
        let deadline: DispatchTime = timeout.isFinite ? .now() + timeout : .distantFuture
        return done.wait(timeout: deadline) == .success ? box.value : nil
    }
```

with:

```swift
    /// A worker's result, and whether its caller stopped waiting for it.
    private final class Job: Sendable {
        private let state = Mutex<(result: String?, abandoned: Bool)>((nil, false))

        var result: String? { state.withLock { $0.result } }
        var isAbandoned: Bool { state.withLock { $0.abandoned } }
        func finish(_ result: String) { state.withLock { $0.result = result } }
        func abandon() { state.withLock { $0.abandoned = true } }
    }

    /// One worker at a time: abandoned parses never pile up.
    private static let workerGate = DispatchSemaphore(value: 1)

    /// Runs `work` on a thread with a 256 MB stack (a virtual reservation) and waits for it, so
    /// libxml's recursive parser cannot exhaust the stack on input the depth guard let through.
    /// Returns `nil` when `timeout` passes first. Workers run one at a time: a worker waits for
    /// the previous one, and skips its work when its caller stopped waiting meanwhile. A worker
    /// already running cannot be cancelled: it finishes and its late result is discarded.
    static func onLargeStack(timeout: TimeInterval = .infinity, _ work: @escaping @Sendable () -> String) -> String? {
        let job = Job()
        let done = DispatchSemaphore(value: 0)
        let thread = Thread {
            workerGate.wait()
            if !job.isAbandoned { job.finish(work()) }
            workerGate.signal()
            done.signal()
        }
        thread.stackSize = 256 << 20
        thread.qualityOfService = .userInteractive
        thread.start()
        let deadline: DispatchTime = timeout.isFinite ? .now() + timeout : .distantFuture
        guard done.wait(timeout: deadline) == .success else {
            job.abandon()
            return nil
        }
        return job.result
    }
```

The `@unchecked Sendable` box goes away: `Job` is `Sendable` through its `Mutex`, which `ArticleTemplate.swift` already imports for the memo.

- [ ] **Step 4: Run the sanitiser tests**

Same command as Task 6.3, Step 4 (it includes `SanitiseTimeoutTests`, whose budget-0 parse is now skipped rather than run). Expected: PASS. Then run the test alone three times to check that it is stable:

```bash
cd /Users/willow/Sites/ressac/apps/macos && for i in 1 2 3; do xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:'RessacTests/SanitizerTests/abandonedWorkersNeverOverlap()' 2>&1 | grep -E 'passed|failed' | tail -1; done
```

- [ ] **Step 5: Commit**

```bash
cd /Users/willow/Sites/ressac
git add apps/macos/Ressac/Reading/ArticleTemplate.swift apps/macos/RessacTests/ReaderTests.swift
git commit -m "$(cat <<'EOF'
Sanitiser: one parse worker at a time; abandoned ones skip their turn

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 6.6: The parse cap goes to 512 KB

**Files:**
- Modify: `apps/macos/Ressac/Reading/ArticleTemplate.swift:146`
- Test: `apps/macos/RessacTests/ReaderTests.swift` (new test before `optionalEndTagsStayOnTheParsedPath`)

This task must come after Tasks 6.3 and 6.4. Raising the cap before the guards would let 512 KB of `<b><div>x</b>` reach libxml.

- [ ] **Step 1: Write the failing test**

In `apps/macos/RessacTests/ReaderTests.swift`, insert before `    @Test func optionalEndTagsStayOnTheParsedPath() {`:

```swift
    /// Up to 512 KB the article is parsed; the largest stored one was 358 KB on 15/10/2026.
    @Test func articlesUpTo512KBAreParsed() {
        let paragraph = "<p>" + String(repeating: "Words of an ordinary long article. ", count: 12) + "<b>bold</b></p>\n"
        let big = String(repeating: paragraph, count: 400_000 / paragraph.utf8.count)
        #expect(big.utf8.count > 390_000)
        #expect(ArticleTemplate.sanitize(big + "<!-- \(UUID()) -->", budget: 10).contains("<b>bold</b>"))
        let huge = String(repeating: paragraph, count: 540_000 / paragraph.utf8.count)
        #expect(!ArticleTemplate.sanitize(huge, budget: 10).contains("<b>"))
    }

```

(The UUID comment keeps the memo from answering with an earlier run's result; `budget: 10` keeps the test about the cap, not about the machine's speed.)

- [ ] **Step 2: Run it and watch it fail**

```bash
cd /Users/willow/Sites/ressac/apps/macos && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:'RessacTests/SanitizerTests/articlesUpTo512KBAreParsed()'
```

Expected: FAIL on `… budget: 10).contains("<b>bold</b>")`: 400 KB is over today's 256 KB cap, so the article comes back as plain text.

- [ ] **Step 3: Implement**

In `apps/macos/Ressac/Reading/ArticleTemplate.swift`, replace line 146:

```swift
    private static let maxParsedBytes = 256 * 1024
```

with:

```swift
    /// Past this, the plain-text fallback. Once the guards below keep libxml linear, its worst
    /// measured cost is about 0.85 ms per KB of dense markup (real articles: 0.1), so 512 KB
    /// stays within half the budget; the largest stored article was 358 KB on 15/10/2026.
    private static let maxParsedBytes = 512 * 1024
```

The memo stores only results under this size, so it now holds at most 32 entries of up to 512 KB (plus their output): a few tens of MB at worst.

- [ ] **Step 4: Run the sanitiser tests**

Same command as Task 6.3, Step 4. Expected: PASS (`misnestedEndTagsAreBoundedByTheSizeCapAndBudget`'s 1.2 MB input is still over the cap).

- [ ] **Step 5: Commit**

```bash
cd /Users/willow/Sites/ressac
git add apps/macos/Ressac/Reading/ArticleTemplate.swift apps/macos/RessacTests/ReaderTests.swift
git commit -m "$(cat <<'EOF'
Sanitiser: parse articles up to 512 KB (largest stored: 358 KB)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 6.7: Images keep the size their feed gives, in both looks

**Files:**
- Modify: `apps/macos/Ressac/Reading/ArticleTemplate.swift:304-311` (renderer, `img`), `:328-336` (`mosaic`), `:344` (new members before `safeAttributes`)
- Test: `apps/macos/RessacTests/ReaderTests.swift` (two tests before `optionalEndTagsStayOnTheParsedPath`; one suite appended at the end)

**Interfaces:**
- Produces:
  - `ArticleTemplate.ImageSize: Equatable, Sendable { var width: Int; var height: Int }`
  - `ArticleTemplate.declaredSize(_ attributes: [String: String]) -> ImageSize?`. It reads, in order: the `width` and `height` attributes; `width` and `height` in pixels in `style`; WordPress's `data-orig-size` ("1200,800"); Blogger's `data-original-width` and `data-original-height`. Each value must be a whole number of pixels from 1 to 20,000.
- Consumes: `MosaicRenderer.columns(forPixelWidth:maxColumns:)`, `MosaicRenderer.grid(width:height:columns:)` and `MosaicRenderer.cellHeight` (unchanged). The mosaic handler computes its grid the same way (`MosaicDecoder.thumbnail`), from the decoded image.

The rules:
- **Native:** an image with neither a `width` nor a `height` attribute gets both when another source gives them. An image with only one keeps it as it is (76 images in the database), so the feed's display width is never replaced by a natural size.
- **Antiope:** the mosaic `img` gets `style="min-height: Npx"`, where N is the rows the renderer will draw × 10 PNG pixels. The stylesheet zooms the `img` by `--font-pixel`, so the box is exactly the mosaic's height. It is a minimum, not a size: an image whose real size differs from what its feed declared is drawn at its real grid, never stretched. Once toggled to the original (`zoom: 1`), the minimum is at most 240 × h / w CSS pixels (rows × 10 = 6 × columns × h / w, with 40 columns at most), below the original's displayed height in any column wider than 240 pixels, so it leaves no gap. A failed mosaic hides its `img`, and the minimum with it.

- [ ] **Step 1: Write the failing tests**

In `apps/macos/RessacTests/ReaderTests.swift`, insert before `    @Test func optionalEndTagsStayOnTheParsedPath() {`:

```swift
    /// Space reads the page's height: an image holds its place before it loads when its feed
    /// gives its size, in an attribute pair, a style or WordPress's or Blogger's data attributes.
    @Test func imagesGetTheSizeTheirFeedGives() {
        let html = #"<img src="https://e.org/a.png" style="width: 600px; height:400px"><img src="https://e.org/b.png" data-orig-size="1200,800">"#
            + #"<img src="https://e.org/c.png" data-original-width="640" data-original-height="480"><img src="https://e.org/d.png" width="50%" style="height: 10px">"#
            + #"<img src="https://e.org/e.png" width="320"><img src="https://e.org/f.png">"#
        let clean = ArticleTemplate.sanitize(html)
        #expect(clean.contains(#"<img src="https://e.org/a.png" width="600" height="400" loading="lazy">"#))
        #expect(clean.contains(#"<img src="https://e.org/b.png" width="1200" height="800" loading="lazy">"#))
        #expect(clean.contains(#"<img src="https://e.org/c.png" width="640" height="480" loading="lazy">"#))
        #expect(clean.contains(#"<img src="https://e.org/d.png" width="50%" loading="lazy">"#))  // its own attribute wins
        #expect(clean.contains(#"<img src="https://e.org/e.png" width="320" loading="lazy">"#))
        #expect(clean.contains(#"<img src="https://e.org/f.png" loading="lazy">"#))
        #expect(!clean.contains("style") && !clean.contains("data-"))
        #expect(ArticleTemplate.declaredSize(["width": "600px", "height": " 400 "]) == ArticleTemplate.ImageSize(width: 600, height: 400))
        #expect(ArticleTemplate.declaredSize(["width": "0", "height": "400"]) == nil)
        #expect(ArticleTemplate.declaredSize(["style": "width:100%;height:auto"]) == nil)
    }

    /// Antiope: the mosaic holds the height its renderer will draw.
    @Test func mosaicsHoldTheirHeightWhenTheFeedGivesASize() {
        let clean = ArticleTemplate.sanitize(#"<img src="https://e.org/a.png" width="600" height="400"><img src="https://e.org/b.png">"#, mosaics: true)
        // 600 pixels: 40 columns (the most); 0.6 × 40 × 400 / 600 = 16 rows of 10 font pixels.
        #expect(clean.contains(#"alt="" loading="lazy" style="min-height: 160px"><span class="mosaic-alt">"#))
        #expect(clean.components(separatedBy: "min-height").count == 2)  // the image without a size has none
    }

```

And append at the end of the file, after Task 6.2's `EntryListPagingTests`:

```swift

/// The image box holds its height before the image arrives, in both looks.
@MainActor @Suite struct ImageBoxTests {
    @Test func anImageOfKnownSizeHoldsItsPlace() async throws {
        let content = #"<p>Text.</p><img src="http://127.0.0.1:9/a.png" style="width:600px;height:400px">"#
        for theme in [ReaderTheme.native, ReaderTheme(appearance: .antiope, crt: false)] {
            let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 600, height: 400), styleMask: [.titled], backing: .buffered, defer: false)
            let webView = WKWebView(frame: window.contentLayoutRect)
            window.contentView = webView
            let controller = ReaderController()
            controller.webView = webView
            let coordinator = ArticleWebView.Coordinator(controller: controller)
            webView.navigationDelegate = coordinator
            let html = ArticleTemplate.page(for: entry(content: content), fulltext: nil, showFull: false, style: ReaderStyle(),
                                            colorScheme: .dark, theme: theme)
            controller.load(entryId: 1, html: html)
            #expect(await eventually { controller.loadedEntryId == 1 })
            let height = try await webView.callAsyncJavaScript(
                #"return document.querySelector(".content img").getBoundingClientRect().height"#, contentWorld: .page)
            #expect((height as? Double ?? 0) > 100, "\(theme.appearance)")
        }
    }
}
```

The image never loads: port 9 on the loopback is closed in Native, and in Antiope a plain `WKWebView` has no `ressac-mosaic:` handler. So the measured height is the box the page reserves.

- [ ] **Step 2: Run them and watch them fail**

```bash
cd /Users/willow/Sites/ressac/apps/macos && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/SanitizerTests -only-testing:RessacTests/ImageBoxTests
```

Expected: the test target does not build: `type 'ArticleTemplate' has no member 'declaredSize'`. (Without those three lines: the `a`, `b`, `c` expectations fail, the mosaic test finds no `min-height`, and `anImageOfKnownSizeHoldsItsPlace` fails in both looks, the box being at most a broken-image icon.)

- [ ] **Step 3: Implement the renderer side (re-read `ArticleTemplate.swift` first)**

Replace lines 304-311:

```swift
                let attributes = safeAttributes(of: element, tag: tag, baseURL: baseURL)
                if tag == "img" {
                    guard let source = attributes.first(where: { $0.name == "src" })?.value else { return }
                    if mosaics, let original = LinkPolicy.webURL(source) {
                        let alt = attributes.first(where: { $0.name == "alt" })?.value ?? ""
                        return mosaic(original: original, alt: alt, inLink: inLink)
                    }
                }
```

with:

```swift
                var attributes = safeAttributes(of: element, tag: tag, baseURL: baseURL)
                if tag == "img" {
                    guard let source = attributes.first(where: { $0.name == "src" })?.value else { return }
                    // Its height is known before it loads, so Space never takes the page's end too early.
                    let size = declaredSize(allAttributes(of: element))
                    if mosaics, let original = LinkPolicy.webURL(source) {
                        let alt = attributes.first(where: { $0.name == "alt" })?.value ?? ""
                        return mosaic(original: original, alt: alt, size: size, inLink: inLink)
                    }
                    if let size, !attributes.contains(where: { $0.name == "width" || $0.name == "height" }) {
                        attributes += [("width", String(size.width)), ("height", String(size.height))]
                    }
                }
```

Replace lines 328-336:

```swift
        /// Image `n` as a mosaic box. The box keeps both addresses for the swap and an IMAGE line
        /// with the alt text, shown by the stylesheet when the mosaic fails (`data-failed`).
        private mutating func mosaic(original: URL, alt: String, inLink: Bool) {
            let index = images
            images += 1
            let source = escapeAttribute(MosaicRequest.url(for: original, columns: MosaicRenderer.maxColumns))
            let box = #"id="m\#(index)" data-original="\#(escapeAttribute(original.absoluteString))" data-mosaic="\#(source)""#
            let label = escapeText(alt.isEmpty ? "IMAGE" : "IMAGE \(alt)")
            let content = #"<img src="\#(source)" alt="\#(escapeAttribute(alt))" loading="lazy">"#
```

with:

```swift
        /// Image `n` as a mosaic box. The box keeps both addresses for the swap and an IMAGE line
        /// with the alt text, shown by the stylesheet when the mosaic fails (`data-failed`). With
        /// the original's `size`, the mosaic keeps its height before it arrives: the rows its
        /// renderer will draw, in PNG pixels (one per font pixel), which the stylesheet zooms.
        private mutating func mosaic(original: URL, alt: String, size: ImageSize?, inLink: Bool) {
            let index = images
            images += 1
            let source = escapeAttribute(MosaicRequest.url(for: original, columns: MosaicRenderer.maxColumns))
            let box = #"id="m\#(index)" data-original="\#(escapeAttribute(original.absoluteString))" data-mosaic="\#(source)""#
            let label = escapeText(alt.isEmpty ? "IMAGE" : "IMAGE \(alt)")
            let height = size.map { size in
                let grid = MosaicRenderer.grid(width: size.width, height: size.height,
                                               columns: MosaicRenderer.columns(forPixelWidth: size.width))
                return #" style="min-height: \#(grid.rows * MosaicRenderer.cellHeight)px""#
            } ?? ""
            let content = #"<img src="\#(source)" alt="\#(escapeAttribute(alt))" loading="lazy"\#(height)>"#
```

Read both places back.

- [ ] **Step 4: Implement the size reader**

Insert before line 344:

```swift
    private static func safeAttributes(of element: XMLElement, tag: String, baseURL: URL?) -> [(name: String, value: String)] {
```

the type and the two functions:

```swift
    /// An image's size in CSS pixels, as its feed gives it.
    struct ImageSize: Equatable, Sendable {
        var width: Int
        var height: Int
    }

    /// The size an image's attributes give: `width` and `height`, else `width` and `height` in
    /// pixels in `style`, else WordPress's `data-orig-size` ("1200,800"), else Blogger's
    /// `data-original-width` and `data-original-height`. Nil when none gives both as whole
    /// pixels from 1 to 20,000 (a percentage gives none).
    static func declaredSize(_ attributes: [String: String]) -> ImageSize? {
        func pixels(_ text: Substring?) -> Int? {
            guard var text = text?.trimmingCharacters(in: .whitespaces).lowercased() else { return nil }
            if text.hasSuffix("px") { text.removeLast(2) }
            guard let value = Int(text), (1...20_000).contains(value) else { return nil }
            return value
        }
        func size(_ width: Substring?, _ height: Substring?) -> ImageSize? {
            guard let width = pixels(width), let height = pixels(height) else { return nil }
            return ImageSize(width: width, height: height)
        }
        if let found = size(attributes["width"]?[...], attributes["height"]?[...]) { return found }
        if let style = attributes["style"] {
            var declarations: [String: Substring] = [:]
            for declaration in style.split(separator: ";") {
                let parts = declaration.split(separator: ":", maxSplits: 1)
                if parts.count == 2 { declarations[parts[0].trimmingCharacters(in: .whitespaces).lowercased()] = parts[1] }
            }
            if let found = size(declarations["width"], declarations["height"]) { return found }
        }
        if let pair = attributes["data-orig-size"]?.split(separator: ","), pair.count == 2,
           let found = size(pair[0], pair[1]) { return found }
        return size(attributes["data-original-width"]?[...], attributes["data-original-height"]?[...])
    }

    /// Every attribute of `element`, names lowercased, unfiltered: only for reading sizes.
    private static func allAttributes(of element: XMLElement) -> [String: String] {
        var attributes: [String: String] = [:]
        for attribute in element.attributes ?? [] {
            if let value = attribute.stringValue { attributes[name(of: attribute)] = value }
        }
        return attributes
    }

```

`style` and `data-*` are read, never written: `safeAttributes` still drops them, as the test checks.

- [ ] **Step 5: Run the reader tests, both looks**

```bash
cd /Users/willow/Sites/ressac/apps/macos && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/SanitizerTests -only-testing:RessacTests/ImageBoxTests -only-testing:RessacTests/MosaicMarkupTests -only-testing:RessacTests/ArticlePageTests -only-testing:RessacTests/AntiopePageTests
```

Expected: PASS. `MosaicMarkupTests.imagesBecomeNumberedMosaicBoxes` is unchanged: its image has a `width` but no `height`, so no size, and its expected markup has no `min-height`.

- [ ] **Step 6: Commit**

```bash
cd /Users/willow/Sites/ressac
git add apps/macos/Ressac/Reading/ArticleTemplate.swift apps/macos/RessacTests/ReaderTests.swift
git commit -m "$(cat <<'EOF'
Reader: images keep the size their feed gives; mosaics keep their height

Native images get width and height from style, data-orig-size or
data-original-*; Antiope mosaics get the min-height of their rows, so
Space no longer takes the end of a page whose last image has not loaded.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Verification gate

- [ ] **Rust**

```bash
cd /Users/willow/Sites/ressac
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: no output from `fmt`, no warnings, all tests pass, with the counts of Phase 05 (`ressac` 35, `cli` 19, core lib 260 + 1 ignored, `article_proxy` 1, `contract` 9, `end_to_end` 4). This phase touches no Rust.

- [ ] **Swift**

```bash
cd /Users/willow/Sites/ressac/apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test
```

Expected: `✔ Test run with 343 tests in 82 suites passed`, `** TEST SUCCEEDED **`, zero warnings. That is the 333 of Phase 05 plus this phase's 10 tests (2 in the new `EntryListPagingTests`, 7 in `SanitizerTests`, 1 in the new `ImageBoxTests`), and 80 suites plus the two new ones. Re-checked on 15/10/2026 on a scratch copy with Phases 01 to 05 applied first, in order: every quoted text matched and each "expected failure" was observed.

- [ ] **By hand, on the real app (Release build, real database)**: open page 400 (Infos, All) in Antiope and in Native. Scroll the list with the mouse past row 500: it goes on without a stop, and the sub-page line reads "3/5907"-like. Then read an entry that ends on an image in Antiope and press Space at the end: the mosaic shows before Space moves on.

---

## Notes for the executor

- **Item 4 has a floor.** 3,610 of the 8,125 images in the database carry no size in any form; their box still appears only when they load. Waiting for images in `pageDown` was rejected in sub-project 2a (a hanging image must not keep Space dead), so this phase does not do it.
- **`srcset` is not read**: it gives widths only, and 56 images have nothing else.
- **Sanitising is still synchronous on the main thread** (`ArticleWebView.updateNSView` → `ArticlePage.html()`). With Tasks 6.3, 6.4 and 6.6 the worst measured case is about 0.45 s (512 KB of entities) and real articles take 10 to 50 ms. Moving it off the main thread would change how `ReaderController` loads pages; it is out of scope.
- **The TODO is not edited here** (file budget). The closing phase of the plan should tick, under « Reporté ▸ Lecture », the list item (Phases 05 and 06), the 256 KB cap, the 600 KB sanitising and the image item, and keep the NSTableView warning.
