//! The Antiope grid in the real app: exact cells at every size and scale, and the measured frame times (spec §7.9).
#![cfg(target_os = "linux")]
mod common;
use common::*;

const PALETTE: [[u8; 3]; 8] = [
    [0, 0, 0],
    [255, 0, 0],
    [0, 255, 0],
    [255, 255, 0],
    [0, 0, 255],
    [255, 0, 255],
    [0, 255, 255],
    [255, 255, 255],
];

fn mean(err: &str, variant: &str, step: u32) -> f64 {
    let line = err
        .lines()
        .filter_map(|l| l.strip_prefix("qml: "))
        .find(|l| {
            l.starts_with(&format!("grid: {variant} ")) && l.contains(&format!(" step={step} "))
        })
        .unwrap_or_else(|| panic!("grid: {variant} step={step} missing in {err}"));
    line.split(" mean=")
        .nth(1)
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

#[test]
fn the_painted_grid_is_exact_at_every_size_and_scale() {
    for scale in ["1", "2"] {
        for size in ["15", "20", "25", "30"] {
            let run = Run::new();
            let out = run.launch_with(
                Some(&run.copy_fixture()),
                &[
                    ("RESSAC_RECETTE_WALK", "grid"),
                    ("RESSAC_RECETTE_GRID", "painted"),
                    ("RESSAC_RECETTE_GRID_SIZE", size),
                    ("QT_SCALE_FACTOR", scale),
                    ("QT_QPA_UPDATE_IDLE_TIME", "0"),
                ],
            );
            assert_eq!(
                out.status.code(),
                Some(0),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_clean(&out);
            let img = image::open(run.shots.join("g01-pattern.png"))
                .unwrap()
                .to_rgb8();
            let (px, s): (u32, u32) = (size.parse().unwrap(), scale.parse().unwrap());
            let (cw, x0, y0) = (px * 6 / 10 * s, 23 * px * 6 / 10 * s, px * s);
            for y in y0..y0 + 4 * px * s {
                for x in x0..x0 + 28 * cw {
                    assert!(
                        PALETTE.contains(&img.get_pixel(x, y).0),
                        "{size}px x{scale}: off-palette at {x},{y}"
                    );
                }
            }
            let row = |y: u32| {
                (x0..x0 + 28 * cw)
                    .map(|x| img.get_pixel(x, y).0)
                    .collect::<Vec<_>>()
            };
            for (i, chunk) in row(y0 + px * s / 2).chunks(cw as usize).enumerate() {
                // `█ █ …`: one cell each, exact
                assert!(
                    chunk.iter().all(|p| *p
                        == if i % 2 == 0 {
                            [255, 255, 255]
                        } else {
                            [0, 0, 0]
                        }),
                    "{size}px x{scale} cell {i}"
                );
            }
            // The colour row: every cell holds its colour, sampled at mid-height and mid-cell.
            let rh = px * s;
            let colour_of = |i: usize| PALETTE[(i % 7) + 1];
            let colours = row(y0 + rh + rh / 2);
            for i in 0..28usize {
                assert_eq!(
                    colours[i * cw as usize + cw as usize / 2],
                    colour_of(i),
                    "{size}px x{scale} colour cell {i}"
                );
            }
            // Runs start and end on cell boundaries, on both rows' mid-height scanlines.
            for y in [y0 + rh / 2, y0 + rh + rh / 2] {
                let line = row(y);
                for x in 1..line.len() {
                    if line[x] != line[x - 1] {
                        assert_eq!(
                            x % cw as usize,
                            0,
                            "{size}px x{scale}: transition at {x}, y {y}"
                        );
                    }
                }
            }
            // Overhang: the pixel row just below the `█ █` row is the colour row's first, and the one
            // below the last row (and above the first) is the black background.
            for (i, p) in row(y0 + rh).chunks(cw as usize).enumerate() {
                assert!(
                    p.iter().all(|c| *c == colour_of(i)),
                    "{size}px x{scale}: overhang into cell {i}"
                );
            }
            for y in [y0 - 1, y0 + 4 * rh] {
                assert!(
                    row(y).iter().all(|c| *c == [0, 0, 0]),
                    "{size}px x{scale}: foreground at row {y}"
                );
            }
        }
    }
}

#[test]
fn the_painted_rows_scroll_faster_than_text_per_span() {
    for scale in ["1", "2"] {
        let run = Run::new();
        let out = run.launch_with(
            Some(&run.copy_fixture()),
            &[
                ("RESSAC_RECETTE_WALK", "grid"),
                ("QT_SCALE_FACTOR", scale),
                ("QT_QPA_UPDATE_IDLE_TIME", "0"),
            ],
        );
        let err = String::from_utf8_lossy(&out.stderr).into_owned();
        assert_eq!(out.status.code(), Some(0), "{err}");
        for step in [1, 10] {
            assert!(
                mean(&err, "painted", step) < mean(&err, "text", step),
                "x{scale} step {step}: {err}"
            );
        }
        for line in err
            .lines()
            .filter_map(|l| l.strip_prefix("qml: "))
            .filter(|l| l.starts_with("grid: "))
        {
            println!("x{scale} {line}");
        } // for notes.md
    }
}
