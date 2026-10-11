# Phase 6: Xcode project, app shell, embedded helper

Read the plan index first. Global Constraints apply. Spec §2 (Platform), §4 (Architecture: the `apps/macos/` layout and Bundling) and §9 (Swift Testing).

**Files (5):**
- Create: `apps/macos/project.yml`
- Create: `apps/macos/scripts/embed-helper.sh`
- Create: `apps/macos/Ressac/App/RessacApp.swift`
- Create: `apps/macos/RessacTests/SmokeTests.swift`
- Modify: `.gitignore`

The generated `apps/macos/Ressac.xcodeproj/` is never committed. Every build command below starts with `xcodegen generate`, because the project lists source files and must be regenerated whenever a file is added.

**Test command** used by every Swift phase (add `-only-testing:RessacTests/<Suite>` to narrow it):

```sh
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)/apps/macos" && xcodegen generate --quiet && \
  xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test \
  2>&1 | grep -E '^/.+: error:|✘|Test run|\*\* TEST' 
```

The first run resolves GRDB from GitHub and, from Task 6.2 on, builds `ressac` in release mode: allow several minutes. Later runs are incremental.

### Task 6.1: The xcodegen project, the app shell and the test target

**Interfaces:**
- Consumes: GRDB.swift `7.11.1` (SwiftPM, exact version).
- Produces:
  - target `Ressac`: macOS application, module `Ressac`, bundle id `org.w3cdotorg.ressac`, deployment target 27.0, Swift 6 language mode with complete strict concurrency, warnings as errors, Info.plist generated from `INFOPLIST_KEY_*` build settings;
  - target `RessacTests`: unit-test bundle hosted by `Ressac.app` (xcodegen sets `TEST_HOST` because it depends on the app), Swift Testing, `@testable import Ressac`;
  - scheme `Ressac`, whose test action runs `RessacTests`;
  - `@main struct RessacApp: App`.

- [ ] **Step 1: Ignore the generated project.** Append to `.gitignore`:

```gitignore
# Xcode: the project is generated from apps/macos/project.yml by xcodegen
apps/macos/Ressac.xcodeproj/
apps/macos/build/
DerivedData/
xcuserdata/
*.xcuserstate
```

- [ ] **Step 2: Write the failing test** `apps/macos/RessacTests/SmokeTests.swift`:

```swift
import Foundation
import GRDB
import Testing

@testable import Ressac

struct SmokeTests {
    @Test func grdb_is_linked() throws {
        let db = try DatabaseQueue()
        let one = try db.read { try Int.fetchOne($0, sql: "SELECT 1") }
        #expect(one == 1)
    }
}
```

- [ ] **Step 3: Run it.** Use the test command above. Expected: `xcodegen` stops with `No project spec found at …/apps/macos/project.yml` (there is no project yet), so nothing builds.

- [ ] **Step 4: Write the project spec** `apps/macos/project.yml`:

```yaml
name: Ressac
options:
  minimumXcodeGenVersion: "2.46.0"
  bundleIdPrefix: org.w3cdotorg
  deploymentTarget:
    macOS: "27.0"
  createIntermediateGroups: true
packages:
  GRDB:
    url: https://github.com/groue/GRDB.swift
    exactVersion: 7.11.1
settings:
  base:
    SWIFT_VERSION: "6.0"
    SWIFT_STRICT_CONCURRENCY: complete
    MACOSX_DEPLOYMENT_TARGET: "27.0"
    ENABLE_USER_SCRIPT_SANDBOXING: NO
    CODE_SIGN_IDENTITY: "-"
    CODE_SIGN_STYLE: Manual
    DEVELOPMENT_TEAM: ""
targets:
  Ressac:
    type: application
    platform: macOS
    sources:
      - path: Ressac
    dependencies:
      - package: GRDB
    settings:
      base:
        PRODUCT_BUNDLE_IDENTIFIER: org.w3cdotorg.ressac
        PRODUCT_NAME: Ressac
        MARKETING_VERSION: "0.1.0"
        CURRENT_PROJECT_VERSION: "1"
        GENERATE_INFOPLIST_FILE: YES
        INFOPLIST_KEY_CFBundleDisplayName: Ressac
        INFOPLIST_KEY_LSApplicationCategoryType: public.app-category.news
        INFOPLIST_KEY_NSHumanReadableCopyright: "MIT licence"
        ENABLE_APP_SANDBOX: NO
        ENABLE_HARDENED_RUNTIME: NO
        SWIFT_TREAT_WARNINGS_AS_ERRORS: YES
    scheme:
      testTargets:
        - RessacTests
  RessacTests:
    type: bundle.unit-test
    platform: macOS
    sources:
      - path: RessacTests
    dependencies:
      - target: Ressac
      - package: GRDB
    settings:
      base:
        PRODUCT_BUNDLE_IDENTIFIER: org.w3cdotorg.ressac.tests
        GENERATE_INFOPLIST_FILE: YES
        SWIFT_TREAT_WARNINGS_AS_ERRORS: YES
```

- [ ] **Step 5: Write the app shell** `apps/macos/Ressac/App/RessacApp.swift`:

```swift
import SwiftUI

@main
struct RessacApp: App {
    var body: some Scene {
        WindowGroup("Ressac") {
            Text("Ressac")
                .frame(minWidth: 480, minHeight: 320)
        }
    }
}
```

- [ ] **Step 6: Run.** Same command. Expected: `✔ Test run with 1 test in 1 suite passed` and `** TEST SUCCEEDED **`. The log also shows harmless `com.apple.linkd.autoShortcut` connection errors from the test host; ignore them.

- [ ] **Step 7: Check that git sees no generated file.** `git status --short` lists only `.gitignore`, `apps/macos/project.yml`, `apps/macos/Ressac/` and `apps/macos/RessacTests/`.

- [ ] **Step 8: Commit.**

```sh
git add .gitignore apps/macos/project.yml apps/macos/Ressac/App/RessacApp.swift apps/macos/RessacTests/SmokeTests.swift
git commit -m "macOS app: xcodegen project, app shell and Swift Testing target"
```

### Task 6.2: Build and embed the `ressac` helper

**Interfaces:**
- Consumes: the workspace's `ressac` binary crate (`cargo build --release -p ressac`, run from the repo root).
- Produces: `Ressac.app/Contents/Helpers/ressac`, signed ad hoc, rebuilt on every app build (cargo is incremental, so an unchanged tree costs about a second).

- [ ] **Step 1: Write the failing test.** Insert into `SmokeTests`, above `grdb_is_linked`:

```swift
    @Test func bundled_helper_is_embedded_and_runs() throws {
        let helper = Bundle.main.bundleURL.appending(path: "Contents/Helpers/ressac")
        #expect(FileManager.default.isExecutableFile(atPath: helper.path))
        let process = Process()
        process.executableURL = helper
        process.arguments = ["--version"]
        let out = Pipe()
        process.standardOutput = out
        try process.run()
        process.waitUntilExit()
        let text = String(decoding: out.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
        #expect(process.terminationStatus == 0)
        #expect(text.hasPrefix("ressac "))
    }
```

- [ ] **Step 2: Run.** Expected: `✘ Test bundled_helper_is_embedded_and_runs() … recorded an issue … Expectation failed: FileManager.default.isExecutableFile(atPath: helper.path)` followed by a thrown error (the file does not exist), and `** TEST FAILED **`.

- [ ] **Step 3: Write the build-phase script** `apps/macos/scripts/embed-helper.sh` and make it executable (`chmod +x apps/macos/scripts/embed-helper.sh`):

```sh
#!/bin/sh
# Builds the ressac CLI and embeds it as Ressac.app/Contents/Helpers/ressac (spec §4, Bundling).
# Run by Xcode as a build phase; SRCROOT is apps/macos.
set -eu
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
repo_root="$SRCROOT/../.."
helpers="$TARGET_BUILD_DIR/$CONTENTS_FOLDER_PATH/Helpers"
# Xcode exports SDK and toolchain variables that are meant for Swift, not for cargo.
env -u SDKROOT -u MACOSX_DEPLOYMENT_TARGET -u ARCHS -u LD -u CC -u CXX \
  cargo build --release -p ressac --manifest-path "$repo_root/Cargo.toml"
mkdir -p "$helpers"
cp "$repo_root/target/release/ressac" "$helpers/ressac"
codesign --force --sign - --timestamp=none "$helpers/ressac"
```

- [ ] **Step 4: Register the script.** In `project.yml`, under `targets: Ressac:`, between `settings:` and `scheme:`, add:

```yaml
    postCompileScripts:
      - name: Embed ressac helper
        path: scripts/embed-helper.sh
        basedOnDependencyAnalysis: false
        showEnvVars: false
        outputFiles:
          - $(TARGET_BUILD_DIR)/$(CONTENTS_FOLDER_PATH)/Helpers/ressac
```

The complete `project.yml` is now:

```yaml
name: Ressac
options:
  minimumXcodeGenVersion: "2.46.0"
  bundleIdPrefix: org.w3cdotorg
  deploymentTarget:
    macOS: "27.0"
  createIntermediateGroups: true
packages:
  GRDB:
    url: https://github.com/groue/GRDB.swift
    exactVersion: 7.11.1
settings:
  base:
    SWIFT_VERSION: "6.0"
    SWIFT_STRICT_CONCURRENCY: complete
    MACOSX_DEPLOYMENT_TARGET: "27.0"
    ENABLE_USER_SCRIPT_SANDBOXING: NO
    CODE_SIGN_IDENTITY: "-"
    CODE_SIGN_STYLE: Manual
    DEVELOPMENT_TEAM: ""
targets:
  Ressac:
    type: application
    platform: macOS
    sources:
      - path: Ressac
    dependencies:
      - package: GRDB
    settings:
      base:
        PRODUCT_BUNDLE_IDENTIFIER: org.w3cdotorg.ressac
        PRODUCT_NAME: Ressac
        MARKETING_VERSION: "0.1.0"
        CURRENT_PROJECT_VERSION: "1"
        GENERATE_INFOPLIST_FILE: YES
        INFOPLIST_KEY_CFBundleDisplayName: Ressac
        INFOPLIST_KEY_LSApplicationCategoryType: public.app-category.news
        INFOPLIST_KEY_NSHumanReadableCopyright: "MIT licence"
        ENABLE_APP_SANDBOX: NO
        ENABLE_HARDENED_RUNTIME: NO
        SWIFT_TREAT_WARNINGS_AS_ERRORS: YES
    postCompileScripts:
      - name: Embed ressac helper
        path: scripts/embed-helper.sh
        basedOnDependencyAnalysis: false
        showEnvVars: false
        outputFiles:
          - $(TARGET_BUILD_DIR)/$(CONTENTS_FOLDER_PATH)/Helpers/ressac
    scheme:
      testTargets:
        - RessacTests
  RessacTests:
    type: bundle.unit-test
    platform: macOS
    sources:
      - path: RessacTests
    dependencies:
      - target: Ressac
      - package: GRDB
    settings:
      base:
        PRODUCT_BUNDLE_IDENTIFIER: org.w3cdotorg.ressac.tests
        GENERATE_INFOPLIST_FILE: YES
        SWIFT_TREAT_WARNINGS_AS_ERRORS: YES
```

- [ ] **Step 5: Run.** Expected: `✔ Test run with 2 tests in 1 suite passed`. Then check the signatures:

```sh
APP="$(xcodebuild -project apps/macos/Ressac.xcodeproj -scheme Ressac -showBuildSettings 2>/dev/null | awk '/ BUILT_PRODUCTS_DIR /{print $3}')/Ressac.app"
codesign -dv "$APP/Contents/Helpers/ressac" 2>&1 | grep Signature   # Signature=adhoc
codesign -v --strict "$APP" && echo app-signature-ok
```

- [ ] **Step 6: Commit.**

```sh
git add apps/macos/project.yml apps/macos/scripts/embed-helper.sh apps/macos/RessacTests/SmokeTests.swift
git commit -m "macOS app: embed the ressac helper in Contents/Helpers, signed ad hoc"
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

Expected: the Rust checks pass unchanged, `** TEST SUCCEEDED **` with 2 Swift tests, and `no warnings in our sources`. Push (Global Constraints).

### Notes for the executor

- **Spec keys checked** against xcodegen 2.46.0's `Docs/ProjectSpec.md`: `options.minimumXcodeGenVersion`, `options.bundleIdPrefix`, `options.deploymentTarget`, `options.createIntermediateGroups`, `packages.<name>.url`/`exactVersion`, target `type`/`platform`/`sources`/`dependencies` (`package:`, `target:`), `postCompileScripts` (`name`, `path`, `basedOnDependencyAnalysis`, `showEnvVars`, `outputFiles`) and `scheme.testTargets`. The whole phase was built and tested with Xcode 27.0 (27A266a), Swift 6.4 and the macOS 27 SDK.
- **Swift 6.4 is the compiler, not a language mode.** The language mode is 6, written `SWIFT_VERSION: "6.0"`; complete strict concurrency is already the default there and is spelt out anyway.
- **`ENABLE_USER_SCRIPT_SANDBOXING: NO` is required.** With the user-script sandbox the build phase may only touch declared inputs, and cargo reads the whole workspace.
- **Info.plist keys are inline build settings** (`GENERATE_INFOPLIST_FILE` plus `INFOPLIST_KEY_*`). xcodegen's `info:` key would instead write an `Info.plist` into the source tree on every generation.
- **The helper is signed by the script, before Xcode signs the app.** Xcode's CodeSign step runs after all script phases and refuses nested code that is not signed.
- **`env -u SDKROOT -u MACOSX_DEPLOYMENT_TARGET …`** keeps cargo's environment identical to a terminal build. Xcode exports these variables for Swift; passing them to cargo changes the fingerprints of crates with build scripts and forces a full Rust rebuild every time you switch between the terminal and Xcode.
- **App Sandbox stays off** (`ENABLE_APP_SANDBOX: NO`): the app spawns a helper on a database in `~/Library/Application Support/Ressac` and reads OPML files the user picks.
- **Hosted tests run inside `Ressac.app`.** `Bundle.main` is the app (so the helper test works), and `XCTestConfigurationFilePath` is set in the environment; phase 9 uses it so that the test host never opens the real database.
