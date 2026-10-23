# Phase B9: the CRT screen (optional, one-phase budget)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** with Antiope shown and Settings ▸ "CRT screen" checked (`antiope.crt`), the window gets the Mac's CRT screen: scanlines on every font-pixel row, a vignette, rounded tube corners, a colour bleed on the teletext text, and the reader's CSS bleed; clicks go through, nothing animates, the overlay is redrawn only when the window or the text size changes. If it does not look right or costs more than this one phase, the toggle is left out and `TODO.md` says why (spec §7.10).

**Runs after:** B8e (VM checkpoint 2, read by Clément). Master plan `docs/plans/2026-10-23-linux.md` (row B9; Global Constraints: Antiope, Settings keys, Language, Never, Gates). Spec §7.9, §7.10 (no Increase Contrast on Plasma: the bleed follows the toggle), §8.5. Inventory §5.8, §7.3 (the toggle and its note), §10.1 `CRTTests`. Mac source: `apps/macos/Ressac/Antiope/CRTOverlay.swift`, `Resources/reader.css:536-541` (the reader's bleed, reused in place). Phase B8's file `docs/plans/linux/phase-b8-antiope.md` (the measured feasibility, `teletext::crt`, `CellRow`, `Antiope`, `AntiopeView.qml`, walks `antiope-scroll`, `marghe.sh`).

**Architecture:** the geometry is already Rust (`ressac_ui::teletext::crt`, B8a1: constants, `is_active`, `scanline_bands`, `corner_radius`) and the header already keeps its right margin clear of the tube corner (`pages::frame::right_margin_cells`, B8b3). B9 draws: a QML `Canvas` overlay (`CrtOverlay.qml`) over `AntiopeView`, painting once per size change the scanline bands the `Antiope` bridge computes, the vignette and the corners; the teletext bleed inside `CellRow`'s `QPainter` pass; the reader's bleed through B3's `ReaderTheme.crt` (`data-crt` on the page, `reader.css`'s `text-shadow`). No shader: B8's planning measured that `ShaderEffect` and `MultiEffect` draw nothing with the software backend, while `Canvas` works there and on the VM's virgl.

**Tech Stack:** Rust 2024 / 1.99; cxx-qt / cxx-qt-lib 0.10.0 (`QPainter`, `QColor` with alpha, `QList<f64>`); Qt 6.11.2 Quick (`Canvas`, `Context2D`); Fedora 44 container; marghe's VM through `apps/linux/dev/marghe.sh`.

**Spec:** `docs/specs/2026-10-23-linux-design.md`, with `docs/specs/linux/mac-app-inventory.md` (binding numbers: opacity 0.28 over the bottom 40 % of each font-pixel row, at least one device pixel; vignette 0 → 0.35 from the centre to the half diagonal; corner radius 3 % of the shorter side; bleed red +1 and blue −1 at 0.45 over a 2-point glow).

## Budget and cut criterion

One phase: B9a (Task 1) then B9b (Task 2), at most **two fix rounds per sub-phase**. The CRT is kept only if every point holds:

1. **Correct offscreen** (container, software renderer, scales 1 and 2, sizes 15 and 20): the walk test's pixel checks pass (bands darken white cells to 183 ± 4 per channel on band rows and leave the rows between at 255 − vignette; a corner pixel is black; the centre is lighter than a point near a corner; red pixels just right of white glyph edges and blue just left when the bleed is on, none when off); a click on an index row through the overlay still jumps to its page.
2. **Cheap:** scrolling the 2,000-entry column (walk `antiope-scroll`), CRT on against CRT off, raises the p95 frame time by at most **2 ms** at scale 2 in the container, with no frame over **33 ms**. (B8's planning measured +0.4 ms for the overlay alone; the bleed adds three text passes per row.)
3. **Looks right on the VM:** when Clément's Plasma session is open, `marghe.sh shots crt-gpu --look antiope --walk crt` shows even scanlines without moiré at scale 2, a soft vignette, round corners, readable text; when it is not, this point moves to B10's recette checklist ("CRT: keep or cut") and the toggle stays until his answer.
4. **Within budget:** a third fix round in either sub-phase, or a failed point 1 or 2 that two rounds do not fix, cuts it.

**Cut** = one `git revert --no-edit <B9a commit>` (its own commit, gate green), no Settings toggle, `Antiope.crt` stays bound to `false`, and in `TODO.md` under « ### Linux (sous-projet 8) » the line (French, no em dash):

`- [ ] Appli KDE : l'écran CRT d'Antiope (lignes de balayage, vignette, coins du tube, bavure des couleurs ; réglage antiope.crt) n'est pas porté : <raison mesurée en B9, chiffres compris>. Pistes : un ShaderEffect avec des shaders .qsb (paquet qt6-qtshadertools) sur le rendu OpenGL de la VM, ou une bavure pré-calculée par ligne peinte (spec §7.10).`

plus a « ### Écran CRT (B9) » paragraph in `notes.md` with the numbers. A hand-edited `antiope.crt=true` then changes nothing (the settings key stays readable, as on the Mac).

## Deviations from the master plan

1. **Two sub-phases** (5-file cap): B9a draws, B9b measures, decides and adds the Settings toggle (so a cut never leaves a toggle behind).
2. **No Increase Contrast:** Plasma has none (spec §7.10); `CRTGeometry.bleedIsActive` and `CRTTests.increaseContrastTurnsTheBleedOff` are not ported, and the toggle's note drops its second sentence: "Scanlines, a vignette and color bleed over the Antiope window."
3. **The glow is approximated**, not blurred: cxx-qt-lib's `QPainter` has no blur. The text is drawn four times in its own colour at alpha 0.25, offset by one device pixel up, down, left and right (≈ the Mac's 2-point glow at scale 2), then the red and blue copies, then the text. The bleed stays inside each row's texture (a row item cannot paint outside itself): the Mac's `displayPadding` has no equivalent, which can clip one pixel of glow at a row's top and bottom.
4. **Bands are computed in Rust** (`Antiope.scanlineBands`), never re-implemented in JavaScript; the canvas only fills them.

## File map

| File | Sub-phase / task | Responsibility |
|---|---|---|
| `apps/linux/src/cellrow.rs` | B9a T1 | `bleed` property and its paint pass |
| `apps/linux/src/reader.rs` | B9a T1 | `crt` property → `ReaderTheme.crt` |
| `apps/linux/src/antiope.rs` | B9a T1 | `scanlineBands(height, pixel, scale) -> QList<f64>` |
| `apps/linux/qml/CrtOverlay.qml` (new) | B9a T1 | scanlines, vignette, corners |
| `apps/linux/qml/AntiopeView.qml` | B9a T1 | the overlay, `Antiope.crt`, `CellRow.bleed`, `Reader.crt` bindings |
| `apps/linux/qml/SettingsWindow.qml` | B9b T2 | "CRT screen" toggle and note |
| `apps/linux/qml/Main.qml` | B9b T2 | walk `crt` |
| `notes.md`, `TODO.md` | B9b T2 | numbers and verdict; the cut line if cut |
| tests (not counted) | | `apps/linux/tests/crt.rs` (new) |

Files per sub-phase: B9a 5, B9b 4.

## Interfaces

- **Consumes:** `teletext::crt::{is_active, scanline_bands, corner_radius, SCANLINE_OPACITY, VIGNETTE_OPACITY, BLEED_OFFSET, BLEED_OPACITY, GLOW_RADIUS}`, `teletext::font_pixel`; `CellRow` (B8b1) and its paint rule; `Antiope.crt` (bound to `false` in B8b5), `pages::frame::right_margin_cells` (already fed by `Antiope.crt`); B3's `ReaderTheme { appearance, crt }` and `ArticlePage::same_page` (the theme is part of the page identity, so a toggle reloads the reader page); B6's `SettingsWindow.qml`; `AppSettings.crt`; B8's walks and `tests/common` (`has_colour`, `insert_entries`), `marghe.sh shots … --look --walk --big`.
- **Produces:** `CellRow.bleed: bool`; `Reader.crt: bool` (`WRITE`); `Antiope.scanlineBands(height: f64, pixel: f64, scale: f64) -> QList<f64>` (flattened `top, bottom` pairs); `CrtOverlay.qml` (`property real textSize`, `enabled: false`); walk `crt`.

## Global Constraints

- **Antiope:** the 8 colours on the text cells (the bleed and the overlay add translucent black, red and blue on top: allowed, as on the Mac, where they are decoration); sizes 15/20/25/30.
- **Settings keys:** `antiope.crt` (Bool, false).
- **Language:** the toggle "CRT screen" and its note in English; `notes.md` and `TODO.md` in French, no em dash in French prose.
- **Never:** the real database; the network in tests; push, PR or merge.
- **Commits:** one per task on branch `linux`, messages ending with the two attribution lines.
- **Gates (end of each sub-phase):**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$HOME/.orbstack/bin:$PATH"
cd /Users/willow/Sites/ressac
cargo fmt --all --check                                   # no output, exit 0
cargo clippy --workspace --all-targets -- -D warnings     # ends with "Finished", no warning
cargo test --workspace                                    # every "test result: ok."
node --test 'apps/macos/ChartTests/*.test.cjs'            # "# fail 0"
apps/linux/dev/dev.sh check                               # last line "check-linux: OK (fmt, clippy, test, chart, layout smoke)"
```

## Review Focus

1. **A click or a wheel scroll over the overlay** (index row, entries list, a link in the reader, the Minitel keys) → it reaches the item under it, as with CRT off. Tests: Task 2 walk step `e04-click-through` (the overlay is a disabled item), and the real mouse on the VM (the B10 recette checklist).
2. **Resizing the window or changing the text size with CRT on** → bands stay on font-pixel rows (no drift after a resize), corners follow the new size; scrolling never repaints the overlay. Tests: Task 2 steps `e05-resized` (bands at the new height), the frame-time run (no overlay repaint: `CrtOverlay` counts its `onPaint` calls, logged as `paints=`, 1 after a 600-frame scroll).
3. **Switching CRT or the look while reading** → the reader page reloads once with or without `data-crt`, the scroll position resets only then; marking read still never reloads. Test: Task 2 step `e06-toggle` (`page=` changes once, `data-crt` present then absent).
4. **CRT on in Native** (a hand-edited `antiope.crt=true` with `appearance=native`) → no overlay, no bleed, no `data-crt`. Test: Task 2 step `e07-native`.
5. **Scale 2 moiré** (bands of half a font pixel at 15 px) → whole device pixels only. Test: Task 1's reuse of `bands_land_on_font_pixel_rows_at_every_text_size` (B8a1) plus Task 2's pixel check at 15 px, scale 2.

---

## B9a: drawing the screen

### Task 1 (B9a): bleed in `CellRow`, `Reader.crt`, `Antiope.scanlineBands`, `CrtOverlay.qml`

**Files:**
- Modify: `apps/linux/src/cellrow.rs`
- Modify: `apps/linux/src/reader.rs`
- Modify: `apps/linux/src/antiope.rs`
- Create: `apps/linux/qml/CrtOverlay.qml`
- Modify: `apps/linux/qml/AntiopeView.qml`

**Interfaces:**
- Consumes: as above.
- Produces: as above.

Behaviour:
- `CellRow.bleed` (`WRITE` → `update()`): in pass 2 (text), for each run with a foreground: four glow copies in the run's colour at alpha `0.25 × 255` offset by ±1 device pixel (`1 / devicePixelRatio` logical) horizontally and vertically; a red copy (`#FF0000`, alpha `BLEED_OPACITY × 255`) at `+BLEED_OFFSET`; a blue copy (`#0000FF`, same alpha) at `−BLEED_OFFSET`; then the text. Replacement runs bleed magenta; backgrounds never bleed.
- `Reader.crt` (`WRITE`): stored into the page's `ReaderTheme { appearance: look, crt }`; a change that alters `same_page` bumps the revision (B3's rule), so the page reloads once.
- `Antiope.scanlineBands(height, pixel, scale)` = `teletext::crt::scanline_bands` flattened into a `QList<f64>`.
- `CrtOverlay.qml`: an `Item` with `enabled: false` (no input) filling `AntiopeView`, holding a `Canvas` (`renderTarget: Canvas.Image`, `renderStrategy: Canvas.Immediate`) repainted only on `width`, `height`, `textSize` or `Screen.devicePixelRatio` changes (`onWidthChanged: requestPaint()` …), counting `property int paints`. `onPaint`: clear; fill every band `Antiope.scanlineBands(height, textSize / 10, Screen.devicePixelRatio)` with `rgba(0,0,0,0.28)` across the width; a radial gradient centred, radius half the diagonal, stops `rgba(0,0,0,0)` → `rgba(0,0,0,0.35)`, over the whole canvas; the corners: `ctx.fillStyle = "black"; ctx.beginPath(); ctx.rect(0, 0, w, h); ctx.roundedRect(0, 0, w, h, r, r); ctx.fillRule = Qt.OddEvenFill; ctx.fill()` with `r = min(w, h) × 3 / 100` (the fill-rule property, not an argument: `fill("evenodd")` paints everything black, seen in B8's planning).
- `AntiopeView.qml`: `readonly property bool crtOn: AppSettings.crt` (the view exists only in Antiope); `Binding { target: Antiope; property: "crt"; value: view.crtOn }` (replacing B8's `false`); every `CellRow` of the view gets `bleed: view.crtOn`; `Reader.crt: view.crtOn` while the view exists (`false` when it is destroyed: Native never shows the bleed); `CrtOverlay { anchors.fill: parent; z: 1000; visible: view.crtOn; textSize: view.size }` last in the view.

- [ ] **Step 1: Write the failing check** — `apps/linux/dev/dev.sh sh 'cargo build --locked -p ressac-kde 2>&1 | grep -E "^error" | head -3'` after declaring `bleed`, `crt` and `scanlineBands` in the bridges only → missing setters / getters.
- [ ] **Step 2: Implement** the five files.
- [ ] **Step 3: Verify** — `apps/linux/dev/dev.sh sh 'cargo clippy --locked -p ressac-kde --all-targets -- -D warnings 2>&1 | tail -1; cargo test --locked -p ressac-kde 2>&1 | grep "test result"'` → `Finished`, every `ok.` (CRT is off in every existing walk). Then the full gate (B9a ends here).
- [ ] **Step 4: Commit**

```bash
git add apps/linux/src/cellrow.rs apps/linux/src/reader.rs apps/linux/src/antiope.rs apps/linux/qml/CrtOverlay.qml apps/linux/qml/AntiopeView.qml
git commit -m "ressac-kde: CRT screen for Antiope (scanlines, vignette, tube corners, colour bleed)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

## B9b: measure, look, decide, toggle

### Task 2 (B9b): walk `crt`, frame times, the VM look, the verdict, the Settings toggle (or the cut)

**Files:**
- Modify: `apps/linux/qml/Main.qml` (`crtSteps`, chooser `"crt"`)
- Modify: `apps/linux/qml/SettingsWindow.qml` (kept only: the toggle row)
- Modify: `notes.md` (« ### Écran CRT (B9) »)
- Modify (cut only): `TODO.md`
- Test (not counted): `apps/linux/tests/crt.rs`

**Interfaces:**
- Consumes: Task 1, B8's `antiope-scroll` walk and `insert_entries`.
- Produces: the kept or cut CRT.

Walk `crtSteps` (the test's copy has entry titles in white cells; logs one `walk:` line per step):

| Step | Action | Log |
|---|---|---|
| `e01-off` | `appearance = "antiope"`, `crt = false` | `crt=false paints=0` |
| `e02-on` | `crt = true` | `crt=true paints=1 page=<Reader.pageUrl>` |
| `e03-bleed` | select feed 1's first entry | `bleed=true` |
| `e04-click-through` | dial `201` with `Recette.key` while the overlay covers the window; read `CrtOverlay.enabled` (no mouse hook in this sub-phase: a disabled item takes no pointer event, so the real click goes on the B10 recette checklist) | `page=201 overlayEnabled=false` |
| `e05-resized` | window width − 100 | `paints=2 bands=<count> lastBottom=<px>` |
| `e06-toggle` | `crt = false` then `true` | `pages=<distinct pageUrl count over the step>=2 dataCrt=true` |
| `e07-native` | `appearance = "native"` with `crt = true` | `overlay=false readerCrt=false` |

`crtScrollSteps` (chooser `"crt-scroll"`): `antiope-scroll`'s probe run twice, CRT off then on, logging `frames: crt=off step=1|10 …` and `frames: crt=on step=1|10 …`, then `walk: paints=<n>`.

- [ ] **Step 1: Write the failing test** `apps/linux/tests/crt.rs`:

```rust
//! The CRT screen (spec §7.10; inventory §5.8): pixels, clicks, cost.
#![cfg(target_os = "linux")]
mod common;
use common::*;

#[test]
fn the_screen_draws_bands_vignette_corners_and_bleed() {
    for (scale, size) in [("1", "20"), ("2", "15"), ("2", "20")] {
        let run = Run::new();
        let out = run.launch_with(Some(&run.copy_fixture()), &[("RESSAC_RECETTE_WALK", "crt"), ("QT_SCALE_FACTOR", scale),
            ("RESSAC_TEST_ANTIOPE_SIZE", size)]);                     // written as reader.size.antiope by the test harness
        let err = String::from_utf8_lossy(&out.stderr).into_owned();
        assert_eq!(out.status.code(), Some(0), "{err}");
        for line in ["walk: e01-off crt=false paints=0", "walk: e02-on crt=true paints=1", "walk: e03-bleed bleed=true",
                     "walk: e04-click-through page=201 overlayEnabled=false", "walk: e07-native overlay=false readerCrt=false"] {
            assert!(err.contains(line), "{line} in {err}");
        }
        let off = image::open(run.shots().join("e01-off.png")).unwrap().to_rgb8();
        let on = image::open(run.shots().join("e03-bleed.png")).unwrap().to_rgb8();
        assert_eq!(on.get_pixel(0, 0).0, [0, 0, 0], "corner");
        // A white cell of the header's RESSAC: band rows darker than the rows between (0.28 black), bleed beside glyphs.
        assert!(darkened_band_rows(&off, &on, size.parse().unwrap(), scale.parse().unwrap()));
        assert!(has_colour_near_white_edges(&on, [255, 0, 0]) && !has_colour_near_white_edges(&off, [255, 0, 0]));
    }
}

#[test]
fn the_screen_costs_at_most_two_milliseconds_per_frame() {
    let run = Run::new();
    let db = run.copy_fixture();
    insert_entries(&db, 1, 2000);
    let out = run.launch_with(Some(&db), &[("RESSAC_RECETTE_WALK", "crt-scroll"), ("QT_SCALE_FACTOR", "2"), ("QT_QPA_UPDATE_IDLE_TIME", "0")]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    for step in [1, 10] {
        let (off, on) = (p95(&err, "off", step), p95(&err, "on", step));
        assert!(on - off <= 2.0, "step {step}: {off} → {on}");
        assert_eq!(over33(&err, "on", step), 0);
        println!("step {step}: p95 {off:.2} → {on:.2}");
    }
    assert!(err.contains("walk: paints=1"), "the overlay never repaints while scrolling: {err}");
}
```

  (`darkened_band_rows`, `has_colour_near_white_edges`, `p95`, `over33` in `tests/common/mod.rs`; `RESSAC_TEST_ANTIOPE_SIZE` handled by `Run::launch_with` writing `reader.size.antiope` into the run's own `ressac-kde.conf` under its temporary `XDG_CONFIG_HOME`.)
- [ ] **Step 2: Run it to verify it fails** — `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde --test crt 2>&1 | grep -E "panicked|test result"'` → FAIL (no `crt` walk).
- [ ] **Step 3: Implement** the walks (`Main.qml`).
- [ ] **Step 4: Measure** — `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde --test crt -- --nocapture 2>&1 | grep -E "step|test result"'` → both `ok.` (criteria 1 and 2) and the p95 pairs. Look at `e03-bleed.png` at scale 2 (bands even, corners round, bleed thin). A failure gets at most two fix rounds (criterion 4).
- [ ] **Step 5: VM look (criterion 3)** — `apps/linux/dev/marghe.sh check`; with `plasma session: yes`: `apps/linux/dev/marghe.sh push && apps/linux/dev/marghe.sh install && apps/linux/dev/marghe.sh shots crt-gpu --look antiope --walk crt` (and `crt-sw … --software-rendering` if checkpoint 2 imposed software rendering); look at `e03-bleed.png` from the VM. With `no`: write « à voir pendant la recette » in the verdict and add "CRT: keep or cut" to the B10 recette checklist (ledger). Either way, add "CRT on: click an index row and scroll the entries with the wheel through the screen" to that checklist. Do not wait for Clément here.
- [ ] **Step 6: Decide.**
  - **Kept** (criteria 1, 2, 4 met; 3 met or deferred): add to `SettingsWindow.qml`'s Reading page, after the Appearance row, `QQC2.CheckBox { text: "CRT screen"; checked: AppSettings.crt; enabled: AppSettings.effectiveAppearance === "antiope"; onToggled: AppSettings.crt = checked }` and the note `QQC2.Label { text: "Scanlines, a vignette and color bleed over the Antiope window."; font: Kirigami.Theme.smallFont; color: Kirigami.Theme.disabledTextColor; wrapMode: Text.Wrap }`; extend B8's `c10-settings` log with `crt=<enabled>`; `notes.md` « ### Écran CRT (B9) » (French, no em dash): kept, the p95 pairs, the pixel checks, the VM look (or deferred), the glow approximation.
  - **Cut:** `git revert --no-edit <the Task 1 commit>` (gate green), no toggle, the `TODO.md` line of "Budget and cut criterion", and the `notes.md` paragraph with the reason and numbers; `tests/crt.rs` is deleted in the same commit as the notes (it tests code that no longer exists).
  Then the full gate (B9b and B9 end here).
- [ ] **Step 7: Commit** (kept):

```bash
git add apps/linux/qml/Main.qml apps/linux/qml/SettingsWindow.qml notes.md apps/linux/tests/crt.rs apps/linux/tests/common/mod.rs
git commit -m "ressac-kde: CRT screen measured and kept, Settings toggle

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

  or (cut), after the revert commit:

```bash
git add apps/linux/qml/Main.qml notes.md TODO.md
git rm apps/linux/tests/crt.rs
git commit -m "Linux: CRT screen left out (measured in B9), TODO line

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

  Ledger: B9a/B9b rows with the verdict (kept / cut), the numbers, fix rounds used.
