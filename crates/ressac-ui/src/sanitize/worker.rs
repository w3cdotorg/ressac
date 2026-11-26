//! The sanitiser's parse workers: threads with a large stack, at most `MAX_WORKERS` at a time
//! through a `Gate`, each waited for within a budget; and the memo of the last results.

use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use super::{Images, MAX_WORKERS, MEMO_CAPACITY, WORKER_STACK_BYTES};

// MARK: Memo

/// The last `MEMO_CAPACITY` results, least recently used first.
pub(super) struct Memo {
    entries: Vec<MemoEntry>,
    hits: u64,
    misses: u64,
}

struct MemoEntry {
    html: String,
    base: Option<String>,
    images: Images,
    value: String,
}

static MEMO: Mutex<Memo> = Mutex::new(Memo {
    entries: Vec::new(),
    hits: 0,
    misses: 0,
});

pub(super) fn memo() -> MutexGuard<'static, Memo> {
    lock(&MEMO)
}

impl Memo {
    fn position(&self, html: &str, base: Option<&str>, images: Images) -> Option<usize> {
        self.entries
            .iter()
            .position(|e| e.images == images && e.base.as_deref() == base && e.html == html)
    }

    pub(super) fn lookup(
        &mut self,
        html: &str,
        base: Option<&str>,
        images: Images,
    ) -> Option<String> {
        let Some(index) = self.position(html, base, images) else {
            self.misses += 1;
            return None;
        };
        self.hits += 1;
        let entry = self.entries.remove(index);
        let value = entry.value.clone();
        self.entries.push(entry);
        Some(value)
    }

    pub(super) fn store(&mut self, html: &str, base: Option<&str>, images: Images, value: &str) {
        if let Some(index) = self.position(html, base, images) {
            self.entries.remove(index);
        } else if self.entries.len() >= MEMO_CAPACITY {
            self.entries.remove(0);
        }
        self.entries.push(MemoEntry {
            html: html.to_owned(),
            base: base.map(str::to_owned),
            images,
            value: value.to_owned(),
        });
    }
}

/// The memo's (hits, misses), for tests.
pub fn memo_stats() -> (u64, u64) {
    let memo = memo();
    (memo.hits, memo.misses)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

// MARK: Large-stack workers

/// A counting semaphore: at most `slots` workers run at once, so abandoned parses never pile up.
pub struct Gate {
    free: Mutex<usize>,
    freed: Condvar,
}

impl Gate {
    pub fn new(slots: usize) -> Arc<Gate> {
        Arc::new(Gate {
            free: Mutex::new(slots),
            freed: Condvar::new(),
        })
    }

    fn enter(&self) -> Slot<'_> {
        let mut free = lock(&self.free);
        while *free == 0 {
            free = self
                .freed
                .wait(free)
                .unwrap_or_else(PoisonError::into_inner);
        }
        *free -= 1;
        Slot(self)
    }
}

/// A taken slot, given back when dropped (also when the work panics).
struct Slot<'a>(&'a Gate);

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        *lock(&self.0.free) += 1;
        self.0.freed.notify_one();
    }
}

/// The gate every sanitiser parse passes through: `MAX_WORKERS` slots.
pub fn shared_gate() -> &'static Arc<Gate> {
    static GATE: OnceLock<Arc<Gate>> = OnceLock::new();
    GATE.get_or_init(|| Gate::new(MAX_WORKERS))
}

/// A worker's result, whether it is over, and whether its caller stopped waiting.
struct Job<T> {
    state: Mutex<JobState<T>>,
    over: Condvar,
}

struct JobState<T> {
    result: Option<T>,
    over: bool,
    abandoned: bool,
}

/// Marks the job over when dropped, so a panicking worker still wakes its caller.
struct Over<'a, T>(&'a Job<T>);

impl<T> Drop for Over<'_, T> {
    fn drop(&mut self) {
        lock(&self.0.state).over = true;
        self.0.over.notify_all();
    }
}

/// Runs `work` on a new thread with a `WORKER_STACK_BYTES` stack and waits up to `timeout` for
/// it. The worker first takes a slot of `gate`, and skips its work when its caller stopped
/// waiting meanwhile. A worker already running cannot be cancelled: it finishes and its late
/// result is dropped. `None` when the time runs out, the work panics or no thread starts.
pub fn on_large_stack<T: Send + 'static>(
    timeout: Duration,
    gate: &Arc<Gate>,
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let job = Arc::new(Job {
        state: Mutex::new(JobState {
            result: None,
            over: false,
            abandoned: false,
        }),
        over: Condvar::new(),
    });
    let worker_job = Arc::clone(&job);
    let worker_gate = Arc::clone(gate);
    let spawned = std::thread::Builder::new()
        .name("ressac-sanitize".into())
        .stack_size(WORKER_STACK_BYTES)
        .spawn(move || {
            // Declared first, dropped last: the slot is free before the caller wakes.
            let _over = Over(&worker_job);
            let _slot = worker_gate.enter();
            if !lock(&worker_job.state).abandoned {
                let result = work();
                lock(&worker_job.state).result = Some(result);
            }
        });
    if spawned.is_err() {
        return None;
    }
    let deadline = Instant::now().checked_add(timeout);
    let mut state = lock(&job.state);
    loop {
        let now = Instant::now();
        if let Some(deadline) = deadline
            && now >= deadline
        {
            state.abandoned = true;
            return None;
        }
        if state.over {
            return state.result.take();
        }
        state = match deadline {
            Some(deadline) => {
                job.over
                    .wait_timeout(state, deadline - now)
                    .unwrap_or_else(PoisonError::into_inner)
                    .0
            }
            None => job.over.wait(state).unwrap_or_else(PoisonError::into_inner),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sanitize::{Images, sanitize, sanitize_within};
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

    const ARTICLE: &str = "https://blog.example.org/2026/10/post.html";

    fn clean(html: &str) -> String {
        sanitize(html, Some(ARTICLE), Images::Web)
    }

    #[test]
    fn abandoned_workers_are_bounded_and_never_block_the_next_article() {
        #[derive(Default)]
        struct Probe {
            running: AtomicUsize,
            peak: AtomicUsize,
            runs: AtomicUsize,
        }
        let probe = Arc::new(Probe::default());
        let gate = Gate::new(MAX_WORKERS);
        let work = {
            let probe = Arc::clone(&probe);
            move || {
                let now = probe.running.fetch_add(1, SeqCst) + 1;
                probe.peak.fetch_max(now, SeqCst);
                probe.runs.fetch_add(1, SeqCst);
                std::thread::sleep(Duration::from_secs(1));
                probe.running.fetch_sub(1, SeqCst);
                "done"
            }
        };
        assert_eq!(
            on_large_stack(Duration::from_millis(300), &gate, work.clone()),
            None
        ); // abandoned while it runs
        // One abandoned worker is running: the next article still gets its worker, within its budget.
        assert_eq!(
            on_large_stack(Duration::from_secs(5), &gate, work.clone()),
            Some("done")
        );
        assert_eq!(probe.peak.load(SeqCst), 2); // it ran beside the abandoned one, not after it
        // Two running (one abandoned), then more callers that give up: they wait, then skip.
        assert_eq!(
            on_large_stack(Duration::from_millis(300), &gate, work.clone()),
            None
        );
        for _ in 0..3 {
            assert_eq!(
                on_large_stack(Duration::from_millis(50), &gate, work.clone()),
                None
            );
        }
        assert_eq!(
            on_large_stack(Duration::from_secs(5), &gate, work),
            Some("done")
        );
        assert!(probe.peak.load(SeqCst) <= 2);
        assert!(probe.runs.load(SeqCst) <= 5);
    }

    #[test]
    fn a_panicking_worker_frees_its_slot_and_gives_nothing() {
        let gate = Gate::new(1);
        assert_eq!(
            on_large_stack(Duration::from_secs(5), &gate, || -> u8 {
                panic!("expected by the test")
            }),
            None
        );
        assert_eq!(on_large_stack(Duration::from_secs(5), &gate, || 7), Some(7));
    }

    #[test]
    fn sanitise_is_memoised() {
        let html = format!("<p>memo test {:?}</p>", std::time::SystemTime::now());
        let first = clean(&html);
        let (hits, _) = memo_stats();
        assert_eq!(clean(&html), first);
        assert!(memo_stats().0 > hits);
    }

    #[test]
    fn a_timed_out_parse_is_not_memoised() {
        let html = "<p>budget <b>test</b> 4242</p>";
        let timed_out = sanitize_within(html, None, Images::Web, Duration::ZERO);
        assert!(timed_out.contains("4242") && !timed_out.contains("<b>")); // the plain-text fallback
        assert!(sanitize(html, None, Images::Web).contains("<b>test</b>")); // not remembered
    }

    #[test]
    fn parse_runs_on_the_large_stack_thread() {
        let caller = std::thread::current().id();
        let worker = on_large_stack(Duration::from_secs(5), shared_gate(), || {
            std::thread::current().id()
        });
        assert!(matches!(worker, Some(id) if id != caller), "{worker:?}");
        assert!(clean("<p>hi</p>").contains("<p>hi"));
    }
}
