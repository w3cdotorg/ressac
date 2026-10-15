# Phase 7: a guarded launch, links through `AppState.openURL`, and a CI that counts its tests

**Runs after:** phase 6, the last of phases 1 to 6, applied in order. Two of the five files below were changed by earlier phases: phase 5 restructures the list handling of `AppState.swift` (668 → 770 lines; `Launcher` moves from line 600 to 702) and appends its `ListPageTests` suite to `AppStateTests.swift`, after `LauncherTests`; phase 6 adds `.onAppear` to the entry rows of `MainView.swift` (line 148, far below `BannerView`). The line numbers below are those after phase 6; the anchors are the quoted texts. Phase 3c changes how `RunCoordinator` fills the banner for a refused garden decision (`showLog: false`), not `BannerView`.

**Goal.** Close the "Appli et outillage" items of `TODO.md` that are not the spam hint, the approved « À décider » item, and the `NSWorkspace` item of the Antiope minors:
- a missing `ressac` helper shows the fatal system alert of spec §8 (`docs/specs/2026-10-10-mac-app-design.md:277`, "Binary missing from the bundle: a fatal alert at launch"), not only a message in the window;
- a second launch (a second window's `.task`, a second click on Use Default Database) is ignored instead of running `migrate` again and replacing the `AppState` on screen;
- the `runs.onDatabaseChange` wiring in `AppState.init` gets a test;
- the native banner's Show Log button and `GardenInspector`'s Open Site and Wayback buttons go through `AppState.openURL`, like the rest of the app;
- CI: the macOS job on the `xcode-27` preview image, so the app is built and tested on every push (decided by Clément on 15/10/2026; `macos-latest` is a one-line rollback), `|| true` on the two Xcode-detection lines (approved by Clément on 15/10/2026), `actions/checkout@v7` (Node 24) in both jobs, and a step that fails when fewer tests ran than `RessacTests` declares.

**Why the `NSWorkspace` item is here and not in phase 8.** Phase 8 (Antiope minors) already needs five files for the formatter cache and the focus fixes. The two `NSWorkspace` call sites are app files (`Reading/`, `Garden/`), not Antiope files, so they join this phase, which has room.

**Files (5):**
- Modify: `apps/macos/Ressac/App/AppState.swift` (`Launcher` only, lines 702 to 770)
- Modify: `apps/macos/RessacTests/AppStateTests.swift`
- Modify: `apps/macos/Ressac/Reading/MainView.swift` (`BannerView`)
- Modify: `apps/macos/Ressac/Garden/GardenInspector.swift`
- Modify: `.github/workflows/ci.yml`

**Step 0.** `AppState.swift` has 770 lines after phase 5, but this phase does not restructure it: it adds two properties, two guards and one static method to `Launcher`. The dead-code audit finds nothing to remove anyway:

```bash
cd /Users/willow/Sites/ressac
wc -l apps/macos/Ressac/App/AppState.swift
grep -nE 'print\(|debugPrint|NSLog|dump\(|TODO|FIXME' apps/macos/Ressac/App/AppState.swift
```

Expected: `770 apps/macos/Ressac/App/AppState.swift`, and the `grep` prints nothing. No Step 0 commit.

**Builds on (exact, current code):**
- `Launcher` (`AppState.swift:702`): `@MainActor @Observable final class`, `private(set) var phase = Phase.starting`, `static var isRunningTests: Bool`, `useDefaultDatabase(defaults:environment:executable:) async`, `launch(database:executable:) async`. `Phase` has no `Equatable` conformance (it carries an `AppState`), so match it with `case`.
- `RessacApp.swift:11-15` calls `launcher.launch(…)` in the `WindowGroup` content's `.task`, once per window. `RessacApp.swift:61` puts the `AppState` in the environment above both looks, so `MainView`'s subviews can read `@Environment(AppState.self)`, as `AntiopeBannerRow` does (`GardenPageViews.swift:130`).
- `AppState.openURL` (`AppState.swift:126`): `@ObservationIgnored var openURL: @MainActor (URL) -> Void`, which tests replace.
- `DatabaseLocation.logsURL` (`SettingsView.swift:15`): `~/Library/Logs/Ressac`, the same folder `BannerView` builds by hand today.
- Test helpers: `makeState()` and `calls(_:)` (`AppStateTests.swift:8` and `:20`), `TestDatabase` (`DataTests.swift:13`; `entry(feed:title:…)` writes through a second connection, `raw`, that GRDB does not observe), `Stub` (`EngineTests.swift:47`; `$2` is the database path, the subcommand is `$4`), `eventually` (`RunCoordinatorTests.swift:8`, 3 s timeout).

**How to run one test.** Swift Testing identifiers work with `-only-testing`:

```bash
cd /Users/willow/Sites/ressac/apps/macos
xcodegen generate
xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test \
  -only-testing:'RessacTests/LauncherTests/missing_helper_is_fatal()'
```

A failing run ends with `** TEST FAILED **` and exit code 65; Swift Testing prints `✘ Test … recorded an issue at AppStateTests.swift:LINE` for each failed expectation.

---

### Task 7.1: a missing helper is a fatal alert (spec §8)

**Files:**
- Modify: `apps/macos/Ressac/App/AppState.swift:712`, `:741-745`, `:766-770`
- Test: `apps/macos/RessacTests/AppStateTests.swift:314-323`

**Interfaces:**
- Produces: `Launcher.presentFatalAlert: @MainActor (String) -> Void` (settable, `@ObservationIgnored`), `static func Launcher.runFatalAlert(_ message: String)`.

The window keeps its `.failed` message behind the alert: `phase` stays `.failed(message)`, so the existing `FirstRunTests.theFailureScreenOffersTheResetAndItClearsThePath` (`FirstRunTests.swift:234`, which relaunches with no helper on a plain `Launcher()`) still sees `offersDefaultDatabase == true`. That test does not replace `presentFatalAlert`; the default, `runFatalAlert`, returns at once in the test host, so it never blocks the run on a modal alert.

- [ ] **Step 1: Write the failing test.** In `AppStateTests.swift`, replace the current test (lines 314 to 323):

```swift
    @Test func missing_helper_is_fatal() async throws {
        let test = try TestDatabase()
        let launcher = Launcher()
        await launcher.launch(database: test.url, executable: nil)
        guard case let .failed(message) = launcher.phase else {
            Issue.record("expected a failure, got \(launcher.phase)")
            return
        }
        #expect(message.hasPrefix("The ressac helper is missing"))
    }
```

with:

```swift
    /// Spec §8: a missing helper is a fatal alert at launch, not only a message in the window.
    @Test func missing_helper_is_fatal() async throws {
        let test = try TestDatabase()
        let launcher = Launcher()
        var alerts: [String] = []
        launcher.presentFatalAlert = { alerts.append($0) }
        await launcher.launch(database: test.url, executable: nil)
        guard case let .failed(message) = launcher.phase else {
            Issue.record("expected a failure, got \(launcher.phase)")
            return
        }
        #expect(message.hasPrefix("The ressac helper is missing"))
        #expect(alerts == [message])
    }
```

- [ ] **Step 2: Run it to see it fail.**

Run: the "How to run one test" command above, with `-only-testing:'RessacTests/LauncherTests/missing_helper_is_fatal()'`.
Expected: the test target does not compile, `error: value of type 'Launcher' has no member 'presentFatalAlert'`; `** TEST FAILED **`.

- [ ] **Step 3: Implement.** In `AppState.swift`, replace line 712:

```swift
    private(set) var phase = Phase.starting
```

with:

```swift
    private(set) var phase = Phase.starting
    /// Shows the fatal alert of spec §8 (the helper is missing). Tests replace it.
    @ObservationIgnored var presentFatalAlert: @MainActor (String) -> Void = { Launcher.runFatalAlert($0) }
```

Replace lines 741 to 745:

```swift
    func launch(database url: URL, executable: URL?) async {
        guard let executable else {
            phase = .failed("The ressac helper is missing from the app bundle. Rebuild the app.")
            return
        }
```

with:

```swift
    func launch(database url: URL, executable: URL?) async {
        guard let executable else {
            let message = "The ressac helper is missing from the app bundle. Rebuild the app."
            phase = .failed(message)
            presentFatalAlert(message)
            return
        }
```

Replace the end of the class, lines 766 to 770:

```swift
        } catch {
            phase = .failed("Cannot open \(url.path): \(error.localizedDescription)")
        }
    }
}
```

with:

```swift
        } catch {
            phase = .failed("Cannot open \(url.path): \(error.localizedDescription)")
        }
    }

    /// The app cannot run a single command without its helper: a critical alert whose only way
    /// out is Quit (spec §8). Never shown in the unit-test host, where it would block the run.
    static func runFatalAlert(_ message: String) {
        guard !isRunningTests else { return }
        let alert = NSAlert()
        alert.alertStyle = .critical
        alert.messageText = "Ressac cannot start"
        alert.informativeText = message
        alert.addButton(withTitle: "Quit Ressac")
        _ = alert.runModal()
        NSApplication.shared.terminate(nil)
    }
}
```

`AppState.swift` already imports `AppKit` (line 1).

- [ ] **Step 4: Run the launcher and first-run suites.**

Run: the same command with `-only-testing:'RessacTests/LauncherTests' -only-testing:'RessacTests/DatabaseChoiceTests'` (`FirstRunTests.swift` has no `FirstRunTests` suite: `theFailureScreenOffersTheResetAndItClearsThePath` is in `DatabaseChoiceTests`).
Expected: `✔ Test run with 9 tests in 2 suites passed`, `** TEST SUCCEEDED **`, no alert on screen, no warning from the Swift compiler (warnings are errors in both targets).

- [ ] **Step 5: Commit.**

```bash
cd /Users/willow/Sites/ressac
git add apps/macos/Ressac/App/AppState.swift apps/macos/RessacTests/AppStateTests.swift
git commit -m "$(cat <<'EOF'
macOS app: a missing ressac helper is a fatal alert at launch (spec §8)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 7.2: one launch at a time

**Files:**
- Modify: `apps/macos/Ressac/App/AppState.swift` (`Launcher`: the properties under `phase`, `useDefaultDatabase`, `launch`)
- Test: `apps/macos/RessacTests/AppStateTests.swift` (`LauncherTests`, after `missing_helper_is_fatal`)

**Interfaces:**
- Consumes: Task 7.1's `presentFatalAlert`.
- Produces: `launch(database:executable:)` returns at once unless `phase` is `.starting` and no launch is running; `useDefaultDatabase(…)` returns at once while a launch runs.

The bug: every window of the `WindowGroup` runs `.task { await launcher.launch(…) }` (`RessacApp.swift:11-15`). File > New is replaced by the app's own items (`Commands.swift:9`), but windows still come and go: close the window with ⌘W and click the Dock icon, and SwiftUI opens a new one whose `.task` launches again; the tab bar's + button does the same. Today that relaunch builds a new `RunCoordinator` and a new `AppState` (the selection is lost, a running refresh is orphaned), and two windows opening together while the database is missing or old run `migrate` twice. A second click on Use Default Database does the same.

- [ ] **Step 1: Write the failing test.** In `AppStateTests.swift`, replace the end of `LauncherTests` as Task 7.1 left it (phase 5's `ListPageTests` follows it, at the end of the file):

```swift
        #expect(alerts == [message])
    }
}
```

with:

```swift
        #expect(alerts == [message])
    }

    /// Two windows launch at once (each `.task` calls `launch`): one `migrate`, one `AppState`.
    /// A launch after that, from a window opened later, keeps the `AppState` already shown.
    @Test func a_second_launch_is_ignored() async throws {
        let test = try TestDatabase()
        let fresh = test.dir.appending(path: "new/ressac.sqlite")
        let stub = try Stub("""
            [ "$4" = migrate ] || exit 2
            echo migrate >> "$(dirname "$0")/calls"
            sleep 0.3
            mkdir -p "$(dirname "$2")" && cp "\(TestDatabase.fixture.path)" "$2"
            echo '{"event":"finished"}'
            """)
        let launcher = Launcher()
        async let first: Void = launcher.launch(database: fresh, executable: stub.executable)
        async let second: Void = launcher.launch(database: fresh, executable: stub.executable)
        _ = await (first, second)
        #expect(try calls(stub) == ["migrate"])
        let state = try #require(launcher.state)
        await launcher.launch(database: fresh, executable: stub.executable)
        #expect(launcher.state === state)
        #expect(try calls(stub) == ["migrate"])
    }
}
```

The stub sleeps so that the first launch is still in `migrate` when the second one starts; it writes the database only in its temp directory.

- [ ] **Step 2: Run it to see it fail.**

Run: `-only-testing:'RessacTests/LauncherTests/a_second_launch_is_ignored()'`.
Expected: `✘ Test a_second_launch_is_ignored() … with 3 issues`: `Expectation failed: try calls(stub) == ["migrate"]` (the calls file holds two `migrate` lines), `Expectation failed: launcher.state === state`, and the last `calls` expectation again; `** TEST FAILED **`.

- [ ] **Step 3: Implement.** In `AppState.swift`, replace:

```swift
    private(set) var phase = Phase.starting
    /// Shows the fatal alert of spec §8 (the helper is missing). Tests replace it.
```

with:

```swift
    private(set) var phase = Phase.starting
    /// True while a launch runs. Each window's `.task` launches (a window reopened from the Dock,
    /// a new tab), and Use Default Database can be clicked twice: a second launch would run
    /// `migrate` again and replace the `AppState` already on screen.
    @ObservationIgnored private var isLaunching = false
    /// Shows the fatal alert of spec §8 (the helper is missing). Tests replace it.
```

Replace (in `useDefaultDatabase`):

```swift
                            executable: URL? = RessacCLI.bundledExecutable()) async {
        DatabaseLocation.resetToDefault(defaults: defaults)
```

with:

```swift
                            executable: URL? = RessacCLI.bundledExecutable()) async {
        guard !isLaunching else { return }
        DatabaseLocation.resetToDefault(defaults: defaults)
```

Replace:

```swift
    func launch(database url: URL, executable: URL?) async {
        guard let executable else {
```

with:

```swift
    /// Launches from `.starting` only, one launch at a time: a later call (another window's
    /// `.task`) leaves the running or finished launch alone.
    func launch(database url: URL, executable: URL?) async {
        guard !isLaunching, case .starting = phase else { return }
        isLaunching = true
        defer { isLaunching = false }
        guard let executable else {
```

`useDefaultDatabase` still sets `phase = .starting` before it calls `launch`, so the reset from `.failed` and `.tooNew` keeps working.

- [ ] **Step 4: Run every suite that creates a `Launcher`.**

Run: `-only-testing:'RessacTests/LauncherTests' -only-testing:'RessacTests/DatabaseChoiceTests' -only-testing:'RessacTests/AppearanceSwitchTests' -only-testing:'RessacTests/FirstRunWALTests'`.
Expected: `✔ Test run with 14 tests in 4 suites passed`, `** TEST SUCCEEDED **`.

- [ ] **Step 5: Commit.**

```bash
cd /Users/willow/Sites/ressac
git add apps/macos/Ressac/App/AppState.swift apps/macos/RessacTests/AppStateTests.swift
git commit -m "$(cat <<'EOF'
macOS app: launch once, whatever the number of windows or clicks

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 7.3: a test for the `onDatabaseChange` wiring

**Files:**
- Test: `apps/macos/RessacTests/AppStateTests.swift` (a new suite between `AppStateTests` and `LauncherTests`)

**Interfaces:**
- Consumes: `AppState.init` (`AppState.swift:159-161`), `refreshAll() -> Task<RunOutcome, Never>`, `RunOutcome.succeeded`.

The wiring exists (`AppState.swift:159-161`); only the test is missing. The test therefore passes as soon as it is written, so Step 2 proves that it can fail by removing the wiring for one run.

- [ ] **Step 1: Write the test.** In `AppStateTests.swift`, replace (the end of `AppStateTests` and the start of `LauncherTests`, lines 255 to 258 before this phase):

```swift
    }
}

@MainActor @Suite struct LauncherTests {
```

with:

```swift
    }
}

/// `AppState.init` wires `runs.onDatabaseChange` to `notifyExternalChanges()`. Rows written by
/// another connection (the `ressac` process, here `TestDatabase.raw`) reach the lists after a run
/// with no other nudge: nothing else notifies in a unit test (`startSystemObservers` is not called).
@MainActor @Suite struct DatabaseChangeWiringTests {
    @Test func a_finished_run_reloads_the_lists() async throws {
        let (state, test, _) = try makeState()
        let id = try test.entry(feed: 1, title: "Written by ressac")
        #expect(state.entries.isEmpty)
        #expect(await state.refreshAll().value == .succeeded)
        #expect(await eventually { state.entries.map(\.entryId) == [id] })
    }
}

@MainActor @Suite struct LauncherTests {
```

- [ ] **Step 2: Prove the test can fail.** Temporarily comment out the wiring in `AppState.swift` (do not commit this):

```swift
        // runs.onDatabaseChange = { [database] in
        //     Task { try? await database.notifyExternalChanges() }
        // }
```

Run: `-only-testing:'RessacTests/DatabaseChangeWiringTests'`.
Expected: after about 3 s, `✘ Test a_finished_run_reloads_the_lists() recorded an issue … Expectation failed: await eventually { state.entries.map(\.entryId) == [id] }`; `** TEST FAILED **`.

Then restore the three lines exactly as they were:

```swift
        runs.onDatabaseChange = { [database] in
            Task { try? await database.notifyExternalChanges() }
        }
```

and check that `git diff apps/macos/Ressac/App/AppState.swift` is empty.

- [ ] **Step 3: Run it to see it pass.**

Run: `-only-testing:'RessacTests/DatabaseChangeWiringTests'`.
Expected: `** TEST SUCCEEDED **`.

- [ ] **Step 4: Commit.**

```bash
cd /Users/willow/Sites/ressac
git add apps/macos/RessacTests/AppStateTests.swift
git commit -m "$(cat <<'EOF'
macOS app: test that a finished run reloads the lists (onDatabaseChange wiring)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 7.4: the native banner and `GardenInspector` open links through `AppState.openURL`

**Files:**
- Modify: `apps/macos/Ressac/Reading/MainView.swift:64-77`
- Modify: `apps/macos/Ressac/Garden/GardenInspector.swift:102-109`

**Interfaces:**
- Consumes: `AppState.openURL`, `DatabaseLocation.logsURL`.

There is no in-process test for this task: both calls sit in view bodies (a button's action), which the test target cannot press without UI automation, and simulated input is ruled out. The verification is the `grep` in Step 3 and the build; the Antiope twins of these actions are already tested through `openURL` (`GardenStoreTests.gardenCommandsActAsInTheNativeTable`, `AntiopeGardenTests.swift:378`).

- [ ] **Step 1: The native banner.** In `MainView.swift`, replace lines 64 to 77:

```swift
struct BannerView: View {
    let banner: Banner
    let dismiss: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.yellow)
            Text(banner.message).lineLimit(2).textSelection(.enabled)
            Spacer()
            if banner.showLog {
                Button("Show Log") {
                    NSWorkspace.shared.open(URL.homeDirectory.appending(path: "Library/Logs/Ressac"))
                }
            }
```

with:

```swift
struct BannerView: View {
    let banner: Banner
    let dismiss: () -> Void

    @Environment(AppState.self) private var state

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.yellow)
            Text(banner.message).lineLimit(2).textSelection(.enabled)
            Spacer()
            if banner.showLog {
                Button("Show Log") { state.openURL(DatabaseLocation.logsURL) }
            }
```

`BannerView` has one caller, `MainView.swift:58`, inside the `.environment(state)` of `RessacApp.swift:61`, like `AntiopeBannerRow`.

- [ ] **Step 2: The garden inspector.** In `GardenInspector.swift`, replace lines 102 to 108:

```swift
            KeyHintButton("Open Site", key: "o", prominent: false) {
                LinkPolicy.webURL(row.htmlUrl).map { _ = NSWorkspace.shared.open($0) }
            }
            .disabled(LinkPolicy.webURL(row.htmlUrl) == nil)
            KeyHintButton("Wayback", key: "w", prominent: false) {
                LinkPolicy.webURL(row.waybackUrl).map { _ = NSWorkspace.shared.open($0) }
            }
```

with:

```swift
            KeyHintButton("Open Site", key: "o", prominent: false) {
                if let site = LinkPolicy.webURL(row.htmlUrl) { state.openURL(site) }
            }
            .disabled(LinkPolicy.webURL(row.htmlUrl) == nil)
            KeyHintButton("Wayback", key: "w", prominent: false) {
                if let wayback = LinkPolicy.webURL(row.waybackUrl) { state.openURL(wayback) }
            }
```

(`if let`, not `.map(state.openURL)`: an unused `()?` result would be a warning, and warnings are errors.)

- [ ] **Step 3: Check that no view calls `NSWorkspace` for these links any more, and build.**

```bash
cd /Users/willow/Sites/ressac
grep -rn 'NSWorkspace.shared.open(' apps/macos/Ressac
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' build
```

Expected: the `grep` prints exactly three lines, all out of scope (see Notes): `Reading/ReaderView.swift:388`, `Settings/SettingsView.swift:313` and `App/AppState.swift:126` (the default of `openURL` itself). The build ends with `** BUILD SUCCEEDED **` and no warning.

- [ ] **Step 4: Commit.**

```bash
cd /Users/willow/Sites/ressac
git add apps/macos/Ressac/Reading/MainView.swift apps/macos/Ressac/Garden/GardenInspector.swift
git commit -m "$(cat <<'EOF'
macOS app: the native banner and the garden inspector open links through AppState.openURL

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 7.5: CI on Xcode 27, `|| true` on the Xcode detection, checkout on Node 24, and a test count

**Files:**
- Modify: `.github/workflows/ci.yml:9`, `:18-19`, `:21`, `:35-36`, `:55-58`

**Facts checked on 15/10/2026** (with `gh api`):
- `actions/checkout`: `v4` runs `using: node20`; `v5`, `v6` and `v7` run `using: node24`. `v5.0.0` (11/08/2025) was the move to Node 24; `v7` is the current major (`v7.0.1`, 20/07/2026). Its breaking change, "block checking out fork pr for pull_request_target and workflow_run", does not concern this workflow (`push` and `pull_request` only). It needs runner 2.327.1 or later, which GitHub-hosted runners have.
- The other actions need nothing: `Swatinem/rust-cache@v2` already runs `using: "node24"` (v2.9.2), and `dtolnay/rust-toolchain@stable` is a composite action.
- `macos-latest` is macOS 26 arm64 with Xcode 26.6 by default, so the macOS job skips the build and tests there. Xcode 27 exists on GitHub as the **preview** image `xcode-27` (`runs-on: xcode-27`, arm64, macOS 27.0.1 since 16/09/2026; Xcode 27.0 27A266a by default, 27.1 and 27.2 beta installed; runner-images issue #14404). Clément decided on 15/10/2026 to move the macOS job to it, so the app is built and its tests run on every push, the new count step included. The detection step stays as the safety net: if the image ever lacks Xcode 27, the job falls back to the allowed failure.
- What a run reports: `xcrun xcresulttool get test-results summary --path <bundle> --compact` prints `totalTestCount`, `passedTests`, `failedTests`, `skippedTests` and `result`. With Xcode 27.0 (27A266a, the build the `xcode-27` image has as default) a parameterized `@Test` counts as **one** test case: before this phase, 318 `@Test` declarations gave `totalTestCount` 318, with two parameterized tests of four arguments each. A declaration is `@Test` at the start of a line, possibly after other attributes (`@MainActor @Test func …` in `ReaderTests.swift:297`).

- [ ] **Step 1: Edit the workflow.** In `.github/workflows/ci.yml`, replace lines 18-19 (job `macos`):

```yaml
    name: macOS app
    runs-on: macos-latest
```

with:

```yaml
    name: macOS app
    runs-on: xcode-27
```

Then replace line 9 (job `check`) and line 21 (job `macos`), each:

```yaml
      - uses: actions/checkout@v4
```

with:

```yaml
      - uses: actions/checkout@v7
```

Replace lines 35 and 36:

```yaml
          xcode=$(xcodebuild -version | head -n 1)
          sdk=$(xcrun --sdk macosx --show-sdk-version)
```

with:

```yaml
          xcode=$(xcodebuild -version | head -n 1) || true
          sdk=$(xcrun --sdk macosx --show-sdk-version) || true
```

With `set -euo pipefail`, a failing `xcodebuild` would otherwise end the step; with `|| true` the variables are empty, the `-ge` tests fail, and the step reports `available=false` with its warning.

Replace lines 55 to 58:

```yaml
      - name: Build and test the app
        if: steps.xcode.outputs.available == 'true'
        working-directory: apps/macos
        run: xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test
```

with:

```yaml
      - name: Build and test the app
        if: steps.xcode.outputs.available == 'true'
        working-directory: apps/macos
        run: xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' -resultBundlePath "$RUNNER_TEMP/TestResults.xcresult" test
      - name: Check that every test ran
        if: steps.xcode.outputs.available == 'true'
        working-directory: apps/macos
        shell: bash
        run: |
          set -euo pipefail
          # One test case per @Test function, a parameterized one included (Xcode 27's count).
          declared=$(grep -rhE '^\s*(@[A-Za-z]+(\([^)]*\))?\s+)*@Test\b' RessacTests | wc -l | tr -d ' ')
          summary=$(xcrun xcresulttool get test-results summary --path "$RUNNER_TEMP/TestResults.xcresult" --compact)
          total=$(jq -r '.totalTestCount' <<<"$summary")
          failed=$(jq -r '.failedTests' <<<"$summary")
          skipped=$(jq -r '.skippedTests' <<<"$summary")
          echo "RessacTests declares $declared tests; $total ran ($failed failed, $skipped skipped)."
          if [ "$total" -lt "$declared" ] || [ "$failed" -ne 0 ]; then
            echo "::error title=Tests missing::$total of the $declared tests declared in RessacTests ran, $failed failed."
            exit 1
          fi
```

The result bundle goes to `$RUNNER_TEMP`, outside the checkout (`xcodebuild` refuses an existing bundle path, and a fresh runner has none). `jq` is installed on the macOS images (1.8.2 on `macos-26` and `xcode-27`). A test skipped by `.enabled(if:)` still counts in `totalTestCount`; the step prints the skipped count rather than failing on it.

- [ ] **Step 2: Lint the workflow.**

```bash
cd /Users/willow/Sites/ressac
export PATH="/opt/homebrew/bin:$PATH"
# actionlint (1.7.12) does not list the preview label xcode-27: declare it for this run only.
cfg=$(mktemp) && printf 'self-hosted-runner:\n  labels:\n    - xcode-27\n' > "$cfg"
actionlint -config-file "$cfg" -ignore SC2012 .github/workflows/ci.yml && echo ACTIONLINT_OK
grep -c 'actions/checkout@v7' .github/workflows/ci.yml
grep -c 'actions/checkout@v4' .github/workflows/ci.yml
grep -c 'runs-on: xcode-27' .github/workflows/ci.yml
```

Expected: `ACTIONLINT_OK`; then `2`, then `0`, then `1`. Without the config file, actionlint 1.7.12 reports `label "xcode-27" is unknown [runner-label]`: the label is GitHub's (a preview image), not a self-hosted one, but actionlint's list stops at `macos-26`. The config stays out of the repository (a sixth file). (`SC2012` is a shellcheck *info* on the existing `ls -d /Applications/Xcode_…` line, present before this phase and out of scope; nothing else may be reported.)

The new step first runs on GitHub with the first push of this commit; the verification gate dry-runs it locally against a real result bundle, made with the same Xcode major as the `xcode-27` image.

- [ ] **Step 3: Commit.**

```bash
cd /Users/willow/Sites/ressac
git add .github/workflows/ci.yml
git commit -m "$(cat <<'EOF'
CI: the macOS job on the xcode-27 image, checkout v7 (Node 24), Xcode detection that cannot end the step, and a check that every test ran

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Verification gate (phase 7)

- [ ] **Rust:**

```bash
cd /Users/willow/Sites/ressac
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: no output from `fmt`, no clippy warning, all tests pass, with the counts of phase 6 (`ressac` 35, `cli` 19, core lib 260 + 1 ignored, `article_proxy` 1, `contract` 9, `end_to_end` 4); this phase changes no Rust.

- [ ] **Swift, keeping a result bundle for the next check:**

```bash
cd /Users/willow/Sites/ressac/apps/macos
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
export GATE_TMP=$(mktemp -d)
xcodegen generate
xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' \
  -resultBundlePath "$GATE_TMP/TestResults.xcresult" test 2>&1 | tee "$GATE_TMP/test.log" | tail -3
grep -E '\.swift:[0-9]+:[0-9]+: warning:' "$GATE_TMP/test.log"
```

Expected: `✔ Test run with 345 tests in 83 suites passed`, `** TEST SUCCEEDED **`; the last `grep` prints nothing (zero Swift warnings).

- [ ] **Dry-run the CI count step** (Task 7.5) on that bundle, in the same shell (it reads `$GATE_TMP`):

```bash
cd /Users/willow/Sites/ressac
export PATH="/opt/homebrew/bin:$PATH"
check=$(mktemp)
ruby -ryaml -e 'puts YAML.load_file(".github/workflows/ci.yml")["jobs"]["macos"]["steps"].find { |s| s["name"] == "Check that every test ran" }["run"]' > "$check"
(cd apps/macos && RUNNER_TEMP="$GATE_TMP" bash "$check"; echo "exit $?")
```

Expected: `RessacTests declares 345 tests; 345 ran (0 failed, 0 skipped).`, then `exit 0`. 345 is the 343 of phase 6 plus the two of Tasks 7.2 and 7.3. Checked on 15/10/2026 with Xcode 27.0 (27A266a), the default of the `xcode-27` image, after phases 1 to 6 applied in order. If the two numbers differ, the count rule of Task 7.5 no longer holds for this Xcode and the step must be revisited, not loosened. (Checked while writing this plan: on a bundle with 318 tests run and 322 declared, the step prints `::error title=Tests missing::318 of the 322 tests declared in RessacTests ran, 0 failed.` and exits 1.)

- [ ] **Workflow:** Task 7.5 Step 2 again: `ACTIONLINT_OK`.

- [ ] **Manual check by Clément (no simulated keystrokes):**
  1. Build the app in Release as the README says (section « The Mac app », into `apps/macos/build/Ressac.app`), move `apps/macos/build/Ressac.app/Contents/Helpers/ressac` aside, and open the app: a critical alert "Ressac cannot start" with a single Quit Ressac button; Quit ends the app. Put the helper back.
  2. Open the app, select an entry, close the window with ⌘W, click the Dock icon: the new window shows the same selected entry (one `AppState`, not a fresh launch).
  3. Native look, a failed refresh banner: Show Log opens `~/Library/Logs/Ressac` in the Finder. Garden, a feed with a site: Open Site and Wayback open the browser.

---

### Notes for the executor

- **Out of scope, still calling `NSWorkspace` directly:** `ReaderView.swift:388` (links clicked in the article, inside the `WKNavigationDelegate` coordinator, which holds no `AppState`) and `SettingsView.swift:313` (Reveal Logs in the Settings scene, which has no `AppState` at all). The TODO names only the banner and `GardenInspector`. `GardenInspector`'s two SwiftUI `Link`s (lines 75 and 78) open through SwiftUI's `OpenURLAction`, not `NSWorkspace`, and stay as they are.
- **`xcode-27` is a preview image** (decided by Clément on 15/10/2026). "Some software can be unstable", jobs may queue, and the first runs may surface CI-only failures (fonts, WebKit) that turn `main` red. **Rollback:** one line, `    runs-on: xcode-27` back to `    runs-on: macos-latest` in the `macos` job; the detection step then skips the build again and the allowed-failure step reports it. Once GitHub moves `macos-latest` (or a `macos-27` image) to Xcode 27, switch to that label.
- **The count rule depends on Xcode.** With Xcode 27.0 a parameterized `@Test` counts as one test case, so `declared` equals `totalTestCount`. If a later default Xcode on the image counts each argument, `total` exceeds `declared` and the step still passes (it only fails on fewer); if it counts fewer, the step fails, and the rule must be revisited, not loosened.
- **TODO.md:** update it in the plan's final docs phase, not here: the « À décider » item, the four « Appli et outillage » items above (the spam hint is another phase's), and the `NSWorkspace` part of the Antiope minors line.
