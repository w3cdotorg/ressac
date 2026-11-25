//! The `Mosaics` QML singleton and the Rust side of the `ressac-mosaic:` scheme. The C++
//! `MosaicHandler` hands each request to `ressac_mosaic_start` with a token; the answer goes
//! back through `ressac_mosaic_reply`, always a PNG made here: the mosaic, or `failure_png()`
//! with the `failed(url)` signal (the handler has no page to script, so QML marks the boxes
//! with `Reader.markFailedScript`). Originals are downloaded by the core's guarded
//! `Ressac::fetch_image` on one mosaic thread, at most `MAX_IN_FLIGHT` at once. Mosaics and
//! lasting failures are cached; transient failures are not.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::QString;
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use ressac_core::service::Ressac;
use ressac_ui::mosaic::{
    MAX_IN_FLIGHT, MosaicCache, MosaicFailure, MosaicRequest, failure_png, fetch, render,
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::launcher::with_session;
use crate::shim::ressac_mosaic_reply;

/// The one type the scheme answers with.
const PNG: &str = "image/png";

/// How long the exit waits for the mosaic thread (a decode in progress finishes first).
const SHUTDOWN_WAIT: Duration = Duration::from_secs(2);

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// Mosaic downloads sent to the mosaic thread and not answered yet.
        #[qproperty(i32, in_flight, READ, NOTIFY)]
        type Mosaics = super::MosaicsRust;

        /// The mosaic address `url` was answered with the failure placeholder: the page marks
        /// its boxes (`Reader.markFailedScript(url)`) so the IMAGE line shows.
        #[qsignal]
        fn failed(self: Pin<&mut Mosaics>, url: &QString);
    }

    impl cxx_qt::Threading for Mosaics {}
    impl cxx_qt::Initialize for Mosaics {}
}

#[derive(Default)]
pub struct MosaicsRust {
    in_flight: i32,
}

impl qobject::Mosaics {
    fn add_in_flight(mut self: Pin<&mut Self>, delta: i32) {
        let count = self.rust().in_flight.saturating_add(delta).max(0);
        if count != self.rust().in_flight {
            self.as_mut().rust_mut().get_mut().in_flight = count;
            self.in_flight_changed();
        }
    }
}

impl cxx_qt::Initialize for qobject::Mosaics {
    fn initialize(self: Pin<&mut Self>) {
        let handle = self.qt_thread();
        LANE.with(|lane| {
            if let Ok(mut lane) = lane.try_borrow_mut() {
                lane.qt = Some(handle);
            }
        });
    }
}

/// One request on its way through the mosaic thread; `reply` is where the answer goes (the
/// `Mosaics` object in the app, nothing in the tests).
struct Job<R> {
    token: u64,
    /// The mosaic address as requested, what `failed` names.
    url: String,
    request: MosaicRequest,
    reply: R,
}

/// The GUI thread's side of the mosaic thread. The thread starts with the first request that
/// needs a download (the database is known only once the launch is ready).
#[derive(Default)]
struct Lane {
    qt: Option<CxxQtThread<qobject::Mosaics>>,
    sender: Option<UnboundedSender<Job<CxxQtThread<qobject::Mosaics>>>>,
    thread: Option<JoinHandle<()>>,
    cache: Arc<Mutex<MosaicCache>>,
}

thread_local! {
    // The scheme handler, `Mosaics` and the exit all run on the GUI thread.
    static LANE: RefCell<Lane> = RefCell::new(Lane::default());
}

/// What `ressac_mosaic_start` does with a request.
enum Start {
    /// Answer now: a cached result, an unparsable address, no way to download.
    Answer { body: Vec<u8>, failed: bool },
    /// Sent to the mosaic thread; the answer is queued back later.
    Sent,
}

impl Start {
    fn of(result: Result<Vec<u8>, MosaicFailure>) -> Start {
        match result {
            Ok(body) => Start::Answer {
                body,
                failed: false,
            },
            Err(_) => Start::failure(),
        }
    }

    fn failure() -> Start {
        Start::Answer {
            body: failure_png(),
            failed: true,
        }
    }
}

fn lock(cache: &Mutex<MosaicCache>) -> MutexGuard<'_, MosaicCache> {
    cache.lock().unwrap_or_else(|e| e.into_inner())
}

/// The recette's walk serves its images from a loopback server: private addresses are
/// allowed only in recette mode (`--screenshot`) with `RESSAC_ALLOW_PRIVATE_ARTICLES=1`.
fn allows_private(recette_active: bool, variable: Option<&str>) -> bool {
    recette_active && variable == Some("1")
}

impl Lane {
    fn start(&mut self, token: u64, url: &str) -> Start {
        let Some(request) = MosaicRequest::parse(url) else {
            return Start::failure();
        };
        if let Some(cached) = lock(&self.cache).get(&request.cache_key()) {
            return Start::of(cached);
        }
        let Some(qt) = self.qt.clone() else {
            tracing::warn!("mosaic: no Mosaics object yet, request {token} gets the placeholder");
            return Start::failure();
        };
        let Some(sender) = self.sender() else {
            return Start::failure();
        };
        let job = Job {
            token,
            url: url.to_string(),
            request,
            reply: qt.clone(),
        };
        if sender.send(job).is_err() {
            tracing::warn!(
                "mosaic: the mosaic thread is gone, request {token} gets the placeholder"
            );
            return Start::failure();
        }
        if qt.queue(|mosaics| mosaics.add_in_flight(1)).is_err() {
            tracing::debug!("mosaic: the app is quitting");
        }
        Start::Sent
    }

    /// The running thread's sender, the thread started (again, after a panic) when needed.
    fn sender(&mut self) -> Option<UnboundedSender<Job<CxxQtThread<qobject::Mosaics>>>> {
        if let Some(sender) = self.sender.as_ref().filter(|sender| !sender.is_closed()) {
            return Some(sender.clone());
        }
        let Some(db) = with_session(|session| session.worker.db().to_path_buf()) else {
            tracing::warn!("mosaic: no database open yet, no download");
            return None;
        };
        let allow_private = allows_private(
            crate::recette::active(),
            std::env::var("RESSAC_ALLOW_PRIVATE_ARTICLES")
                .ok()
                .as_deref(),
        );
        let (sender, receiver) = unbounded_channel();
        let cache = self.cache.clone();
        let spawned = std::thread::Builder::new()
            .name("ressac-mosaic".into())
            .spawn(move || mosaic_thread(&db, allow_private, &cache, receiver));
        match spawned {
            Ok(thread) => {
                // A thread that ended (it panicked) is joined here; its requests stay unanswered.
                if let Some(old) = self.thread.replace(thread) {
                    let _ = old.join();
                }
                self.sender = Some(sender.clone());
                Some(sender)
            }
            Err(error) => {
                tracing::warn!("mosaic: cannot start the mosaic thread: {error}");
                None
            }
        }
    }
}

/// The C++ handler's request `token` for the mosaic address `url` (GUI thread). An unparsable
/// address or a cached result is answered at once; otherwise the mosaic thread downloads and
/// renders, and the answer is queued back. A failure also emits `Mosaics.failed(url)`. Called
/// from C++: it never panics.
pub fn ressac_mosaic_start(token: u64, url: &str) {
    let start = LANE.with(|lane| match lane.try_borrow_mut() {
        Ok(mut lane) => lane.start(token, url),
        Err(_) => Start::failure(),
    });
    let Start::Answer { body, failed } = start else {
        return;
    };
    ressac_mosaic_reply(token, PNG, &body);
    if failed {
        let qt = LANE.with(|lane| lane.try_borrow().ok().and_then(|lane| lane.qt.clone()));
        let url = url.to_string();
        let queued = qt.map(|qt| qt.queue(move |mosaics| mosaics.failed(&QString::from(&url))));
        if !matches!(queued, Some(Ok(()))) {
            tracing::debug!("mosaic: no Mosaics object to tell that {token} failed");
        }
    }
}

/// Ends the mosaic thread at exit, after the QML engine is gone and before the application:
/// the channel closes, the downloads in flight are dropped, and the thread is joined within
/// `SHUTDOWN_WAIT` (one still decoding is logged and left to the process exit; its answer can
/// only queue onto the deleted `Mosaics`, which refuses it).
pub(crate) fn shutdown() -> bool {
    let (sender, thread) = LANE.with(|lane| match lane.try_borrow_mut() {
        Ok(mut lane) => {
            lane.qt = None;
            (lane.sender.take(), lane.thread.take())
        }
        Err(_) => (None, None),
    });
    drop(sender);
    let Some(thread) = thread else {
        return true;
    };
    join_within(thread, SHUTDOWN_WAIT)
}

fn join_within(thread: JoinHandle<()>, wait: Duration) -> bool {
    let deadline = Instant::now() + wait;
    while !thread.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    if thread.is_finished() {
        let _ = thread.join();
        true
    } else {
        tracing::warn!(
            "mosaic: the mosaic thread was still busy {wait:?} after the shutdown began"
        );
        false
    }
}

/// The mosaic thread: its own `Ressac` (a failed open answers every request with the
/// placeholder), each answer queued onto the `Mosaics` the request came with.
fn mosaic_thread(
    db: &Path,
    allow_private: bool,
    cache: &Mutex<MosaicCache>,
    receiver: UnboundedReceiver<Job<CxxQtThread<qobject::Mosaics>>>,
) {
    let app = open_app(db, allow_private);
    run_lane(app.as_ref(), cache, receiver, |job, result| {
        let failed = result.is_err();
        let body = result.unwrap_or_else(|_| failure_png());
        let Job {
            token, url, reply, ..
        } = job;
        let queued = reply.queue(move |mut mosaics| {
            ressac_mosaic_reply(token, PNG, &body);
            mosaics.as_mut().add_in_flight(-1);
            if failed {
                mosaics.failed(&QString::from(&url));
            }
        });
        if queued.is_err() {
            tracing::debug!("mosaic: the app quit before {token} was answered");
        }
    });
}

fn open_app(db: &Path, allow_private: bool) -> Option<Ressac> {
    let opened = Ressac::open(db).and_then(|app| {
        if allow_private {
            app.allow_private_articles()
        } else {
            Ok(app)
        }
    });
    match opened {
        Ok(app) => Some(app),
        Err(error) => {
            tracing::warn!("mosaic: cannot open the database, no download: {error}");
            None
        }
    }
}

/// Runs the jobs from `receiver` until it closes, at most `MAX_IN_FLIGHT` downloads at once,
/// the others waiting in order. Each result is cached (the cache keeps only lasting ones) and
/// then handed to `deliver`, exactly once per job. A closed channel ends the lane at once: the
/// downloads in flight are dropped unanswered.
fn run_lane<R>(
    app: Option<&Ressac>,
    cache: &Mutex<MosaicCache>,
    mut receiver: UnboundedReceiver<Job<R>>,
    mut deliver: impl FnMut(Job<R>, Result<Vec<u8>, MosaicFailure>),
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::warn!("mosaic: no runtime, no download: {error}");
            while let Some(job) = receiver.blocking_recv() {
                deliver(job, Err(MosaicFailure::Network));
            }
            return;
        }
    };
    runtime.block_on(async {
        let mut waiting = VecDeque::new();
        let mut running = FuturesUnordered::new();
        loop {
            while running.len() < MAX_IN_FLIGHT
                && let Some(job) = waiting.pop_front()
            {
                let job: Job<R> = job;
                // Another request for the same image may have finished meanwhile.
                let cached = lock(cache).get(&job.request.cache_key());
                match cached {
                    Some(result) => deliver(job, result),
                    None => running.push(produce(app, job)),
                }
            }
            tokio::select! {
                job = receiver.recv() => match job {
                    Some(job) => waiting.push_back(job),
                    None => break,
                },
                Some((job, result)) = running.next(), if !running.is_empty() => {
                    lock(cache).put(&job.request.cache_key(), result.clone());
                    deliver(job, result);
                }
            }
        }
    });
}

/// Download, then render (the decode is bounded by `ressac_ui::mosaic::decode`'s guards).
async fn produce<R>(app: Option<&Ressac>, job: Job<R>) -> (Job<R>, Result<Vec<u8>, MosaicFailure>) {
    let result = match app {
        Some(app) => fetch(app, &job.request.original)
            .await
            .and_then(|bytes| render(&bytes, job.request.columns)),
        None => Err(MosaicFailure::Network),
    };
    (job, result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;

    use image::{ImageFormat, Rgba, RgbaImage};

    /// The database a test lane opens.
    fn test_db(dir: &Path) -> std::path::PathBuf {
        dir.join("mosaic.db")
    }

    #[test]
    fn private_addresses_need_the_recette_and_the_variable() {
        assert!(allows_private(true, Some("1")));
        assert!(!allows_private(false, Some("1")));
        assert!(!allows_private(true, None));
        assert!(!allows_private(true, Some("yes")));
        assert!(!allows_private(false, None));
    }

    fn encoded(format: ImageFormat) -> Vec<u8> {
        let image = RgbaImage::from_pixel(120, 60, Rgba([255, 0, 0, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        image.write_to(&mut out, format).unwrap();
        out.into_inner()
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    /// A small PNG whose header claims 8000 × 8000 (CRC fixed).
    fn huge_header_png() -> Vec<u8> {
        let mut png = encoded(ImageFormat::Png);
        png[16..20].copy_from_slice(&8000u32.to_be_bytes());
        png[20..24].copy_from_slice(&8000u32.to_be_bytes());
        let crc = crc32(&png[12..29]);
        png[29..33].copy_from_slice(&crc.to_be_bytes());
        png
    }

    /// One answer of the loopback server: status, extra headers, body; `None` never answers.
    type Answer = Option<(u16, Vec<(String, String)>, Vec<u8>)>;

    /// A loopback HTTP/1.1 server: each path answers from its list in turn (the last one
    /// repeats). Counts the connections it accepted.
    struct Server {
        base: String,
        accepted: Arc<AtomicUsize>,
    }

    fn serve(routes: HashMap<&'static str, Vec<Answer>>) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let accepted = Arc::new(AtomicUsize::new(0));
        let counter = accepted.clone();
        let routes = Arc::new(Mutex::new(
            routes
                .into_iter()
                .map(|(path, answers)| (path.to_string(), VecDeque::from(answers)))
                .collect::<HashMap<_, _>>(),
        ));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                counter.fetch_add(1, Ordering::SeqCst);
                let routes = routes.clone();
                std::thread::spawn(move || answer(stream, &routes));
            }
        });
        Server { base, accepted }
    }

    fn answer(mut stream: TcpStream, routes: &Mutex<HashMap<String, VecDeque<Answer>>>) {
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            match stream.read(&mut byte) {
                Ok(1) => head.push(byte[0]),
                _ => return,
            }
        }
        let head = String::from_utf8_lossy(&head);
        let target = head.split(' ').nth(1).unwrap_or("/");
        let path = target.split('?').next().unwrap_or("/").to_string();
        let next = {
            let mut routes = routes.lock().unwrap();
            routes.get_mut(&path).and_then(|answers| {
                if answers.len() > 1 {
                    answers.pop_front()
                } else {
                    answers.front().cloned()
                }
            })
        };
        let Some(next) = next else {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\n\r\n");
            return;
        };
        let Some((status, headers, body)) = next else {
            // Never answers: hold the connection open.
            std::thread::sleep(Duration::from_secs(60));
            return;
        };
        let mut reply = format!(
            "HTTP/1.1 {status} X\r\ncontent-length: {}\r\nconnection: close\r\n",
            body.len()
        );
        for (name, value) in headers {
            reply.push_str(&format!("{name}: {value}\r\n"));
        }
        reply.push_str("\r\n");
        let _ = stream.write_all(reply.as_bytes());
        let _ = stream.write_all(&body);
    }

    fn typed(status: u16, mime: &str, body: Vec<u8>) -> Answer {
        Some((status, vec![("content-type".into(), mime.into())], body))
    }

    fn job(token: u64, original: &str) -> Job<()> {
        let url = MosaicRequest::url_for(original, 40);
        Job {
            token,
            request: MosaicRequest::parse(&url).unwrap(),
            url,
            reply: (),
        }
    }

    /// Runs `paths` through a lane (private addresses allowed) and returns each token's result.
    fn run(
        server: &Server,
        cache: &Mutex<MosaicCache>,
        paths: &[&str],
    ) -> Vec<(u64, Result<Vec<u8>, MosaicFailure>)> {
        let dir = tempfile::tempdir().unwrap();
        let app = open_app(&test_db(dir.path()), true).expect("a test app");
        let (sender, receiver) = unbounded_channel();
        for (token, path) in paths.iter().enumerate() {
            let original = format!("{}{path}", server.base);
            sender.send(job(token as u64, &original)).ok().unwrap();
        }
        // A closed channel ends the lane at once: it stays open until every job is answered.
        let sender = Mutex::new(Some(sender));
        let mut results = Vec::new();
        run_lane(Some(&app), cache, receiver, |job, result| {
            results.push((job.token, result));
            if results.len() == paths.len() {
                sender.lock().unwrap().take();
            }
        });
        results.sort_by_key(|(token, _)| *token);
        results
    }

    #[test]
    fn hostile_images_end_as_a_mosaic_or_a_failure_and_only_lasting_ones_are_cached() {
        let html = b"<html><body>not an image</body></html>".to_vec();
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"/>"#.to_vec();
        let server = serve(HashMap::from([
            (
                "/ok.png",
                vec![typed(200, "image/png", encoded(ImageFormat::Png))],
            ),
            (
                "/anim.gif",
                vec![typed(200, "image/gif", encoded(ImageFormat::Gif))],
            ),
            ("/html.png", vec![typed(200, "image/png", html)]),
            ("/pic.svg", vec![typed(200, "image/svg+xml", svg)]),
            ("/empty.png", vec![typed(200, "image/png", Vec::new())]),
            (
                "/huge.png",
                vec![typed(200, "image/png", huge_header_png())],
            ),
            (
                "/big.png",
                vec![typed(200, "image/png", vec![0u8; 11 << 20])],
            ),
            (
                "/flaky.png",
                vec![
                    typed(503, "text/plain", b"busy".to_vec()),
                    typed(200, "image/png", encoded(ImageFormat::Png)),
                ],
            ),
            (
                "/to-file.png",
                vec![Some((
                    302,
                    vec![("location".into(), "file:///etc/passwd".into())],
                    Vec::new(),
                ))],
            ),
        ]));
        let cache = Mutex::new(MosaicCache::default());
        let paths = [
            "/ok.png",
            "/anim.gif",
            "/html.png",
            "/pic.svg",
            "/empty.png",
            "/huge.png",
            "/big.png",
            "/flaky.png",
            "/to-file.png",
        ];
        let results = run(&server, &cache, &paths);
        let tokens: Vec<u64> = results.iter().map(|(token, _)| *token).collect();
        assert_eq!(
            tokens,
            (0..paths.len() as u64).collect::<Vec<_>>(),
            "one answer each"
        );
        let classes: Vec<Result<(), MosaicFailure>> = results
            .iter()
            .map(|(_, result)| {
                result
                    .as_ref()
                    .map(|png| assert!(png.starts_with(b"\x89PNG")))
            })
            .map(|result| result.map_err(|failure| *failure))
            .collect();
        assert_eq!(classes[0], Ok(()), "png");
        assert_eq!(classes[1], Ok(()), "gif");
        assert_eq!(classes[2], Err(MosaicFailure::Unreadable), "html as png");
        assert!(classes[3].is_err(), "svg: {:?}", classes[3]);
        assert!(classes[4].is_err(), "empty: {:?}", classes[4]);
        assert_eq!(classes[5], Err(MosaicFailure::TooLarge), "8000 x 8000");
        assert_eq!(classes[6], Err(MosaicFailure::TooLarge), "11 MB");
        assert_eq!(classes[7], Err(MosaicFailure::Status(503)), "503 first");
        assert!(classes[8].is_err(), "redirect to file: {:?}", classes[8]);

        let key = |path: &str| {
            MosaicRequest::parse(&MosaicRequest::url_for(
                &format!("{}{path}", server.base),
                40,
            ))
            .unwrap()
            .cache_key()
        };
        let mut cached = lock(&cache);
        assert!(cached.get(&key("/ok.png")).is_some_and(|r| r.is_ok()));
        assert_eq!(
            cached.get(&key("/html.png")),
            Some(Err(MosaicFailure::Unreadable))
        );
        assert_eq!(
            cached.get(&key("/flaky.png")),
            None,
            "a 503 is not remembered"
        );
        drop(cached);

        // The 503 was not cached: asking again downloads again and gets the 200.
        let again = run(&server, &cache, &["/flaky.png"]);
        assert!(again[0].1.is_ok(), "{:?}", again[0].1);
    }

    #[test]
    fn at_most_four_downloads_run_at_once_and_a_closed_lane_ends_without_waiting() {
        let server = serve(HashMap::from([("/hang.png", vec![None])]));
        let dir = tempfile::tempdir().unwrap();
        let db = test_db(dir.path());
        let (sender, receiver) = unbounded_channel();
        let (delivered_tx, delivered_rx) = mpsc::channel();
        let lane = std::thread::spawn(move || {
            let app = open_app(&db, true).expect("a test app");
            let cache = Mutex::new(MosaicCache::default());
            run_lane(Some(&app), &cache, receiver, |job: Job<()>, _| {
                let _ = delivered_tx.send(job.token);
            });
        });
        // Six requests for distinct images that never answer.
        for token in 0..6u64 {
            let original = format!("{}/hang.png?{token}", server.base);
            sender.send(job(token, &original)).ok().unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while server.accepted.load(Ordering::SeqCst) < MAX_IN_FLIGHT && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(server.accepted.load(Ordering::SeqCst), MAX_IN_FLIGHT);

        let closed = Instant::now();
        drop(sender);
        let deadline = closed + Duration::from_secs(2);
        while !lane.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            lane.is_finished(),
            "the lane still runs after its channel closed"
        );
        lane.join().unwrap();
        assert!(
            delivered_rx.try_recv().is_err(),
            "a dropped download is never answered"
        );
    }

    #[test]
    fn join_within_gives_up_on_a_busy_thread() {
        let busy = std::thread::spawn(|| std::thread::sleep(Duration::from_millis(400)));
        let started = Instant::now();
        assert!(!join_within(busy, Duration::from_millis(50)));
        assert!(started.elapsed() < Duration::from_millis(300));
        let quick = std::thread::spawn(|| {});
        assert!(join_within(quick, Duration::from_secs(1)));
    }
}
