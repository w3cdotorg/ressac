//! The `Mosaics` QML singleton and the Rust side of the `ressac-mosaic:` scheme. The C++
//! `MosaicHandler` hands each request to `ressac_mosaic_start` with a token; the answer goes
//! back through `ressac_mosaic_reply`, always a PNG made here: the mosaic, or `failure_png()`
//! with the `failed(url)` signal (the handler has no page to script, so QML marks the boxes
//! with `Reader.markFailedScript`). Originals are downloaded by the core's guarded
//! `Ressac::fetch_image` on one mosaic thread, at most `MAX_IN_FLIGHT` at once. A request the
//! page cancels (`ressac_mosaic_cancel`) leaves the queue, and its download stops if it had
//! started. Mosaics and lasting failures are cached; transient failures are not.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::QString;
use futures::StreamExt;
use futures::future::{AbortHandle, Abortable};
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
        /// Requests sent to the mosaic thread and neither answered nor cancelled yet.
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
    /// `inFlight` from the GUI thread's pending set, the one count: the order in which the
    /// queued updates arrive does not matter.
    fn sync_in_flight(mut self: Pin<&mut Self>) {
        let pending = LANE
            .try_with(|lane| lane.try_borrow().map_or(0, |lane| lane.pending.len()))
            .unwrap_or(0);
        let count = i32::try_from(pending).unwrap_or(i32::MAX);
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

/// What the GUI thread tells the mosaic thread, in order: a cancel always follows its job.
enum Message<R> {
    Job(Job<R>),
    /// The page cancelled request `token`: drop it from the queue, stop its download.
    Cancel(u64),
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
    sender: Option<Sender>,
    thread: Option<JoinHandle<()>>,
    cache: Arc<Mutex<MosaicCache>>,
    /// Tokens sent to the mosaic thread, neither answered nor cancelled yet (`inFlight`).
    pending: HashSet<u64>,
}

type Sender = UnboundedSender<Message<CxxQtThread<qobject::Mosaics>>>;

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
        // Pending before the send: the answer, queued from the mosaic thread, finds it there.
        self.pending.insert(token);
        if sender.send(Message::Job(job)).is_err() {
            self.pending.remove(&token);
            tracing::warn!(
                "mosaic: the mosaic thread is gone, request {token} gets the placeholder"
            );
            return Start::failure();
        }
        queue_sync(&qt);
        Start::Sent
    }

    /// The running thread's sender, the thread started (again, after a panic) when needed.
    fn sender(&mut self) -> Option<Sender> {
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
                    self.pending.clear();
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

/// Queues an `inFlight` update onto `Mosaics`.
fn queue_sync(qt: &CxxQtThread<qobject::Mosaics>) {
    if qt.queue(|mosaics| mosaics.sync_in_flight()).is_err() {
        tracing::debug!("mosaic: the app is quitting");
    }
}

/// The C++ handler's request `token` for the mosaic address `url` (GUI thread). An unparsable
/// address or a cached result is answered at once; otherwise the mosaic thread downloads and
/// renders, and the answer is queued back. A failure also emits `Mosaics.failed(url)`. Called
/// from C++: it never panics.
pub fn ressac_mosaic_start(token: u64, url: &str) {
    let start = LANE
        .try_with(|lane| match lane.try_borrow_mut() {
            Ok(mut lane) => lane.start(token, url),
            Err(_) => Start::failure(),
        })
        .unwrap_or_else(|_| Start::failure());
    let Start::Answer { body, failed } = start else {
        return;
    };
    ressac_mosaic_reply(token, PNG, &body);
    if failed {
        let qt = LANE
            .try_with(|lane| lane.try_borrow().ok().and_then(|lane| lane.qt.clone()))
            .ok()
            .flatten();
        let url = url.to_string();
        let queued = qt.map(|qt| qt.queue(move |mosaics| mosaics.failed(&QString::from(&url))));
        if !matches!(queued, Some(Ok(()))) {
            tracing::debug!("mosaic: no Mosaics object to tell that {token} failed");
        }
    }
}

/// The C++ handler's request `token` was cancelled by its page before its answer (GUI
/// thread): it leaves the queue, its download stops, `inFlight` drops. A token already
/// answered or unknown is ignored. Called from C++: it never panics.
pub fn ressac_mosaic_cancel(token: u64) {
    let _ = LANE.try_with(|lane| {
        let Ok(mut lane) = lane.try_borrow_mut() else {
            return;
        };
        if !lane.pending.remove(&token) {
            return;
        }
        if let Some(sender) = &lane.sender {
            let _ = sender.send(Message::Cancel(token));
        }
        if let Some(qt) = &lane.qt {
            queue_sync(qt);
        }
    });
}

/// Ends the mosaic thread at exit, after the QML engine is gone and before the application:
/// the channel closes, the downloads in flight are dropped, and the thread is joined within
/// `SHUTDOWN_WAIT` (one still decoding is logged and left to the process exit; its answer can
/// only queue onto the deleted `Mosaics`, which refuses it).
pub(crate) fn shutdown() -> bool {
    let (sender, thread) = LANE
        .try_with(|lane| match lane.try_borrow_mut() {
            Ok(mut lane) => {
                lane.qt = None;
                lane.pending.clear();
                (lane.sender.take(), lane.thread.take())
            }
            Err(_) => (None, None),
        })
        .unwrap_or((None, None));
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
    receiver: UnboundedReceiver<Message<CxxQtThread<qobject::Mosaics>>>,
) {
    let app = open_app(db, allow_private);
    run_lane(app.as_ref(), cache, receiver, |job, result| {
        let failed = result.is_err();
        let body = result.unwrap_or_else(|_| failure_png());
        let Job {
            token, url, reply, ..
        } = job;
        let queued = reply.queue(move |mut mosaics| {
            // A request cancelled meanwhile is no longer pending: C++ drops its answer.
            let pending = LANE
                .try_with(|lane| {
                    lane.try_borrow_mut()
                        .is_ok_and(|mut lane| lane.pending.remove(&token))
                })
                .unwrap_or(false);
            ressac_mosaic_reply(token, PNG, &body);
            mosaics.as_mut().sync_in_flight();
            if failed && pending {
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
/// then handed to `deliver`, exactly once per job that was not cancelled. A cancel removes a
/// waiting job and aborts a running download (never answered). A closed channel ends the lane
/// at once: the downloads in flight are dropped unanswered, and the runtime is shut down in
/// the background so a blocking DNS lookup cannot hold the thread.
fn run_lane<R>(
    app: Option<&Ressac>,
    cache: &Mutex<MosaicCache>,
    mut receiver: UnboundedReceiver<Message<R>>,
    mut deliver: impl FnMut(Job<R>, Result<Vec<u8>, MosaicFailure>),
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::warn!("mosaic: no runtime, no download: {error}");
            while let Some(message) = receiver.blocking_recv() {
                if let Message::Job(job) = message {
                    deliver(job, Err(MosaicFailure::Network));
                }
            }
            return;
        }
    };
    runtime.block_on(async {
        let mut waiting: VecDeque<Job<R>> = VecDeque::new();
        let mut running = FuturesUnordered::new();
        let mut aborts: HashMap<u64, AbortHandle> = HashMap::new();
        loop {
            while running.len() < MAX_IN_FLIGHT
                && let Some(job) = waiting.pop_front()
            {
                // Another request for the same image may have finished meanwhile.
                let cached = lock(cache).get(&job.request.cache_key());
                match cached {
                    Some(result) => deliver(job, result),
                    None => {
                        let (abort, registration) = AbortHandle::new_pair();
                        aborts.insert(job.token, abort);
                        running.push(Abortable::new(produce(app, job), registration));
                    }
                }
            }
            tokio::select! {
                // Messages first: the cancels already sent take effect before a freed slot
                // starts the next download.
                biased;
                message = receiver.recv() => match message {
                    Some(Message::Job(job)) => waiting.push_back(job),
                    Some(Message::Cancel(token)) => {
                        waiting.retain(|job| job.token != token);
                        if let Some(abort) = aborts.remove(&token) {
                            abort.abort();
                        }
                    }
                    None => break,
                },
                Some(outcome) = running.next(), if !running.is_empty() => {
                    // An aborted download ends as `Err(Aborted)`: nothing to answer.
                    if let Ok((job, result)) = outcome {
                        aborts.remove(&job.token);
                        lock(cache).put(&job.request.cache_key(), result.clone());
                        deliver(job, result);
                    }
                }
            }
        }
    });
    runtime.shutdown_background();
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

    type Answered = (u64, Result<Vec<u8>, MosaicFailure>);

    /// A lane on its own thread (private addresses allowed), its answers on a channel, so a
    /// lost answer fails the test by `recv_timeout` instead of hanging it.
    struct TestLane {
        sender: Option<UnboundedSender<Message<()>>>,
        answers: mpsc::Receiver<Answered>,
        thread: Option<JoinHandle<()>>,
        _dir: tempfile::TempDir,
    }

    impl TestLane {
        fn start(cache: Arc<Mutex<MosaicCache>>) -> TestLane {
            let dir = tempfile::tempdir().unwrap();
            let db = test_db(dir.path());
            let (sender, receiver) = unbounded_channel();
            let (answer_tx, answers) = mpsc::channel();
            let thread = std::thread::spawn(move || {
                let app = open_app(&db, true).expect("a test app");
                run_lane(Some(&app), &cache, receiver, |job: Job<()>, result| {
                    let _ = answer_tx.send((job.token, result));
                });
            });
            TestLane {
                sender: Some(sender),
                answers,
                thread: Some(thread),
                _dir: dir,
            }
        }

        fn send(&self, token: u64, original: &str) {
            let sender = self.sender.as_ref().unwrap();
            sender
                .send(Message::Job(job(token, original)))
                .ok()
                .unwrap();
        }

        fn cancel(&self, token: u64) {
            let sender = self.sender.as_ref().unwrap();
            sender.send(Message::Cancel(token)).ok().unwrap();
        }

        /// The next answer, or a failed test once `deadline` has passed.
        fn answer_by(&self, deadline: Instant) -> Answered {
            let left = deadline.saturating_duration_since(Instant::now());
            self.answers
                .recv_timeout(left)
                .expect("an answer before the deadline")
        }

        /// Closes the channel; whether the lane thread ended within 2 s.
        fn close(&mut self) -> bool {
            drop(self.sender.take());
            self.thread
                .take()
                .is_none_or(|thread| join_within(thread, Duration::from_secs(2)))
        }
    }

    /// Runs `paths` through a test lane and returns each token's result, all within 30 s.
    fn run(server: &Server, cache: &Arc<Mutex<MosaicCache>>, paths: &[&str]) -> Vec<Answered> {
        let mut lane = TestLane::start(cache.clone());
        for (token, path) in paths.iter().enumerate() {
            lane.send(token as u64, &format!("{}{path}", server.base));
        }
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut results: Vec<Answered> = paths.iter().map(|_| lane.answer_by(deadline)).collect();
        assert!(lane.close(), "the lane did not end");
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
        let cache = Arc::new(Mutex::new(MosaicCache::default()));
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

    /// Waits (5 s at most) until `server` has accepted `count` connections, then a little
    /// longer so that more would show.
    fn settle(server: &Server, count: usize) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while server.accepted.load(Ordering::SeqCst) < count && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(300));
    }

    #[test]
    fn at_most_four_downloads_run_at_once_and_a_closed_lane_ends_without_waiting() {
        let server = serve(HashMap::from([("/hang.png", vec![None])]));
        let mut lane = TestLane::start(Arc::default());
        // Six requests for distinct images that never answer.
        for token in 0..6u64 {
            lane.send(token, &format!("{}/hang.png?{token}", server.base));
        }
        settle(&server, MAX_IN_FLIGHT);
        assert_eq!(server.accepted.load(Ordering::SeqCst), MAX_IN_FLIGHT);
        assert!(lane.close(), "the lane still runs after its channel closed");
        assert!(
            lane.answers.try_recv().is_err(),
            "a dropped download is never answered"
        );
    }

    #[test]
    fn cancelled_requests_leave_the_queue_and_free_their_downloads() {
        let server = serve(HashMap::from([
            ("/hang.png", vec![None]),
            (
                "/ok.png",
                vec![typed(200, "image/png", encoded(ImageFormat::Png))],
            ),
        ]));
        let mut lane = TestLane::start(Arc::default());
        // Four downloads hang, two more wait behind them.
        for token in 0..6u64 {
            lane.send(token, &format!("{}/hang.png?{token}", server.base));
        }
        settle(&server, MAX_IN_FLIGHT);
        assert_eq!(server.accepted.load(Ordering::SeqCst), MAX_IN_FLIGHT);
        // The page goes away: every one is cancelled, the waiting ones first (a slot freed by
        // an abort would otherwise start the next waiting one), then the running ones.
        for token in (0..6u64).rev() {
            lane.cancel(token);
        }
        lane.send(6, &format!("{}/ok.png", server.base));
        let (token, result) = lane.answer_by(Instant::now() + Duration::from_secs(5));
        assert_eq!(token, 6, "only the seventh request is answered");
        assert!(result.is_ok(), "{result:?}");
        // The two waiting ones never started: the hanging server saw only the first four.
        assert_eq!(server.accepted.load(Ordering::SeqCst), MAX_IN_FLIGHT + 1);
        assert!(lane.close());
        assert!(
            lane.answers.try_recv().is_err(),
            "a cancelled request is never answered"
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
