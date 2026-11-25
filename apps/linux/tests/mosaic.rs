//! Mosaics in the Antiope reader (spec §7): an article's images arrive through the
//! `ressac-mosaic:` scheme as teletext mosaics; a click swaps one with its original, `i` swaps
//! them all, a failed image is a magenta `IMAGE <alt>` line, and a request the page cancels
//! stops its download. The images come from a loopback server in this test, never the network.
#![cfg(target_os = "linux")]
mod common;
use common::*;

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Rows of the default Antiope size (`ANTIOPE_SIZE_DEFAULT`), cells 12 px wide.
const SIZE: u32 = 20;
const CELL: u32 = 12;
/// The reader column's first cell: the 22-cell index, a rule, the 28-cell entries, a rule.
const READER: u32 = 22 + 1 + 28 + 1;
/// CSS pixels per font pixel at this size (a cell is 6 font pixels wide).
const FONT_PIXEL: u32 = CELL / 6;
const RED: [u8; 3] = [255, 0, 0];
const MAGENTA: [u8; 3] = [255, 0, 255];
/// How long `/slow.png` takes to answer.
const SLOW: Duration = Duration::from_secs(30);
/// The walk holds this long after the cancel before it ends (`d07`): a connection closed sooner
/// after its request was stopped by the app, not by the exit.
const STOPPED_WITHIN: Duration = Duration::from_millis(2000);

/// What became of the `/slow.png` request.
#[derive(Default)]
struct Slow {
    started: AtomicBool,
    /// How long after the request the client closed the connection, before the answer.
    closed_after: Mutex<Option<Duration>>,
    answered: AtomicBool,
}

/// `/photo.png` (120 × 120 red), `/broken.png` (HTML), `/slow.png` (after 30 s), anything else a
/// 404 page; one request per connection, each on its own thread.
fn serve() -> (u16, Arc<Slow>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let slow = Arc::new(Slow::default());
    let shared = slow.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let slow = shared.clone();
            std::thread::spawn(move || answer(stream, &slow));
        }
    });
    (port, slow)
}

fn red_png() -> Vec<u8> {
    let image = image::RgbImage::from_pixel(120, 120, image::Rgb(RED));
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    png.into_inner()
}

fn respond(stream: &mut TcpStream, status: &str, mime: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

fn answer(mut stream: TcpStream, slow: &Slow) {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut request = Vec::new();
    let mut buf = [0u8; 1024];
    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => request.extend_from_slice(&buf[..n]),
        }
    }
    let line = String::from_utf8_lossy(&request);
    let path = line.split(' ').nth(1).unwrap_or("").to_string();
    match path.as_str() {
        "/photo.png" => respond(&mut stream, "200 OK", "image/png", &red_png()),
        "/broken.png" => respond(
            &mut stream,
            "200 OK",
            "text/html",
            b"<html><body>Not an image</body></html>",
        ),
        "/slow.png" => {
            slow.started.store(true, Ordering::SeqCst);
            let started = Instant::now();
            let closed = || *slow.closed_after.lock().unwrap() = Some(started.elapsed());
            // Waits, watching for the client to hang up: a read that ends means it closed.
            stream
                .set_read_timeout(Some(Duration::from_millis(100)))
                .unwrap();
            let until = Instant::now() + SLOW;
            while Instant::now() < until {
                match stream.read(&mut buf) {
                    Ok(0) => return closed(),
                    Ok(_) => {}
                    Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                    Err(_) => return closed(),
                }
            }
            respond(&mut stream, "200 OK", "image/png", &red_png());
            slow.answered.store(true, Ordering::SeqCst);
        }
        _ => respond(&mut stream, "404 Not Found", "text/html", b"<html></html>"),
    }
}

/// Three entries in feed 1, newer than the fixture's: 9001 shows the photo and the broken image,
/// 9002 the slow one, 9003 a failing address with a space and an `é` (encoded on its way).
fn prepare(db: &std::path::Path, port: u16) {
    let base = format!("http://127.0.0.1:{port}");
    let conn = rusqlite::Connection::open(db).unwrap();
    let mut insert = conn
        .prepare(
            "INSERT INTO entry (id, feed_id, guid, url, title, published_at, content_html, fetched_at)
             VALUES (?1, 1, ?2, ?3, ?4, ?5, ?6, '2026-11-20T08:00:00.000Z')",
        )
        .unwrap();
    for (id, title, published, content) in [
        (
            9001,
            "Mosaic",
            "2026-11-20T03:00:00.000Z",
            format!(
                r#"<p><img src="{base}/photo.png" alt="Photo"></p><p><img src="{base}/broken.png" alt="Gone"></p>"#
            ),
        ),
        (
            9002,
            "Slow",
            "2026-11-20T02:00:00.000Z",
            format!(r#"<p><img src="{base}/slow.png" alt="Slow"></p>"#),
        ),
        (
            9003,
            "Accent",
            "2026-11-20T01:00:00.000Z",
            format!(r#"<p><img src="{base}/un été.png" alt="Accent"></p>"#),
        ),
    ] {
        insert
            .execute(rusqlite::params![
                id,
                format!("mosaic-{id}"),
                format!("http://127.0.0.1:9/mosaic/{id}"),
                title,
                published,
                content,
            ])
            .unwrap();
    }
}

/// The Antiope view's top-left corner in the window (below the menu bar), from the walk's log.
fn origin(err: &str) -> (u32, u32) {
    let line = walk_line(err, "walk: d01-origin ");
    let value = |key: &str| -> u32 {
        let v: f64 = line
            .split(key)
            .nth(1)
            .unwrap()
            .split(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        v.round() as u32
    };
    (value(" x="), value(" y="))
}

/// The bounding box of the `colour` pixels in `rect`, and how many there are.
fn bounds(shot: &image::RgbImage, rect: Rect, colour: [u8; 3]) -> Option<(Rect, usize)> {
    let hits: Vec<(u32, u32)> = rect
        .pixels()
        .filter(|&(x, y)| shot.get_pixel(x, y).0 == colour)
        .collect();
    let (x0, y0) = hits
        .iter()
        .fold((u32::MAX, u32::MAX), |(a, b), &(x, y)| (a.min(x), b.min(y)));
    let (x1, y1) = hits
        .iter()
        .fold((0, 0), |(a, b), &(x, y)| (a.max(x), b.max(y)));
    (!hits.is_empty()).then(|| {
        (
            Rect {
                x: x0,
                y: y0,
                width: x1 - x0 + 1,
                height: y1 - y0 + 1,
            },
            hits.len(),
        )
    })
}

#[test]
fn the_mosaic_walk_swaps_marks_and_cancels() {
    let (port, slow) = serve();
    let run = Run::new();
    let db = run.copy_fixture();
    prepare(&db, port);
    let out = run.launch_with(
        Some(&db),
        &[
            ("TZ", "UTC"),
            ("RESSAC_RECETTE_WALK", "mosaic"),
            ("RESSAC_ALLOW_PRIVATE_ARTICLES", "1"),
            ("QT_LOGGING_RULES", "ressac.mosaic.debug=true"),
        ],
    );
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    for line in [
        // The photo's mosaic: 120 px over 10 columns of 6 font pixels; the broken image (HTML)
        // failed, so its box shows the IMAGE line.
        "walk: d02 m0=60 m1failed=true alt=block".to_string(),
        // A click on the first box shows the original.
        format!("walk: d03 src=http://127.0.0.1:{port}/photo.png original=true"),
        // `i` while a mosaic shows: every original; again: every mosaic.
        "walk: d04 originals=2".to_string(),
        "walk: d05 originals=0".to_string(),
        // The slow image is on its way when another entry is selected.
        "walk: d06 pending=1".to_string(),
    ] {
        let step = line.split(' ').take(2).collect::<Vec<_>>().join(" ");
        assert_eq!(walk_line(&err, &format!("{step} ")), line, "{err}");
    }
    // The cancelled request left the count, and the next page's failure was marked.
    let d07 = walk_line(&err, "walk: d07 ");
    assert!(
        d07.starts_with("walk: d07 inFlight=0 failed=true mosaic=ressac-mosaic:"),
        "{err}"
    );
    // `failed(url)` names the box's `data-mosaic` byte for byte, the encoded space and `é` too.
    let mosaic = d07.split(" mosaic=").nth(1).unwrap();
    assert!(
        mosaic.contains("%2520%25C3%25A9t%25C3%25A9.png"),
        "{mosaic}"
    );
    assert!(
        err.lines()
            .map(|l| l.strip_prefix("qml: ").unwrap_or(l))
            .any(|l| l == format!("mosaic: failed {mosaic}")),
        "{err}"
    );
    // The page cancelled /slow.png: the app stopped its download (the connection closed soon
    // after the request, long before the exit) and never answered it.
    assert!(slow.started.load(Ordering::SeqCst), "{err}");
    let closed_after = slow
        .closed_after
        .lock()
        .unwrap()
        .expect("/slow.png never closed");
    assert!(
        closed_after < STOPPED_WITHIN,
        "closed after {closed_after:?}"
    );
    assert!(!slow.answered.load(Ordering::SeqCst));

    let o = origin(&err);
    let shot = |name: &str| {
        image::open(run.shots.join(format!("{name}.png")))
            .unwrap()
            .to_rgb8()
    };
    // The reader's page, below its title row, above the Minitel bar.
    let page = |image: &image::RgbImage| {
        let x = o.0 + READER * CELL;
        let y = o.1 + 2 * SIZE;
        Rect {
            x,
            y,
            width: image.width() - x,
            height: image.height() - SIZE - y,
        }
    };
    // d01: the photo's mosaic, a solid red block of whole cells, 60 × 60 font pixels; below it
    // the broken image's magenta IMAGE line.
    let d01 = shot("d01-mosaic");
    let area = page(&d01);
    let (block, count) = bounds(&d01, area, RED).expect("d01: no red mosaic");
    let side = 60 * FONT_PIXEL;
    assert_eq!((block.width, block.height), (side, side), "d01: {block:?}");
    assert_eq!(count as u32, side * side, "d01: the block has holes");
    assert_eq!((block.x - area.x) % CELL, 0, "d01: {block:?} off the cells");
    let below = Rect {
        y: block.y + block.height,
        height: area.y + area.height - (block.y + block.height),
        ..area
    };
    assert!(has_colour(&d01, below, MAGENTA), "d01: no IMAGE line");
    // d03: the original photo (120 × 120 CSS pixels), the IMAGE line still below it.
    let d03 = shot("d03-click");
    let (photo, count) = bounds(&d03, page(&d03), RED).expect("d03: no photo");
    assert_eq!((photo.width, photo.height), (120, 120), "d03: {photo:?}");
    assert_eq!(count, 120 * 120);
}
