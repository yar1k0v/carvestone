//! Keeps a long-running server from refreshing the index on every read.
//!
//! A refresh walks every code file and asks git for HEAD, which costs far more than most reads.
//! The CLI pays it once per run; a server gets dozens of reads per page. A `Freshness` attached to
//! the repo (`Repo::with_freshness`) lets one refresh serve them all: concurrent reads wait for
//! the same run, and later reads skip it until something says the tree moved.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::refresh::RefreshReport;
use crate::repo::{EngineError, Repo};

/// The last refresh that finished.
#[derive(Debug, Clone, Copy)]
struct Done {
    at: Instant,
    /// The value of `dirty` when that refresh started.
    dirty: u64,
    /// The history cache was whole.
    history: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Fresh,
    /// Past its age: good enough to answer from, but worth checking.
    Aged,
    Stale,
}

/// When a repo's index needs another refresh.
#[derive(Debug)]
pub struct Freshness {
    max_age: Duration,
    /// A file watcher reports changes, so age alone never makes a read wait.
    watched: bool,
    /// Bumped by `invalidate`: the tree is known to have moved.
    dirty: AtomicU64,
    /// Bumped whenever an answer may differ; caches of answers key on it.
    generation: AtomicU64,
    done: Mutex<Option<Done>>,
    /// Held for the length of a refresh, so only one runs.
    running: Arc<Mutex<()>>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // The guarded values stay valid whatever a panicking refresh left behind.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Freshness {
    /// `max_age` bounds how late a change nothing reports (a new commit) shows up. With `watched`
    /// a read past that age is answered at once and the refresh runs behind it; without, it waits.
    pub fn new(max_age: Duration, watched: bool) -> Arc<Freshness> {
        Arc::new(Freshness {
            max_age,
            watched,
            dirty: AtomicU64::new(0),
            generation: AtomicU64::new(0),
            done: Mutex::new(None),
            running: Arc::new(Mutex::new(())),
        })
    }

    /// The tree moved: the next read refreshes first, and cached answers are out of date.
    pub fn invalidate(&self) {
        self.dirty.fetch_add(1, Ordering::SeqCst);
        self.touch();
    }

    /// Answers may differ although the files did not; the index itself need not refresh.
    pub fn touch(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    /// Changes whenever a cached answer may no longer hold.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// The index needs no refresh right now.
    pub fn is_current(&self) -> bool {
        self.state() == State::Fresh
    }

    fn state(&self) -> State {
        match *locked(&self.done) {
            Some(done) if done.dirty == self.dirty.load(Ordering::SeqCst) => {
                if done.at.elapsed() < self.max_age {
                    State::Fresh
                } else {
                    State::Aged
                }
            }
            _ => State::Stale,
        }
    }

    /// Whether the history cache was whole at the last refresh; `None` when that says nothing.
    pub(crate) fn history_complete(&self) -> Option<bool> {
        match self.state() {
            State::Fresh => locked(&self.done).map(|done| done.history),
            State::Aged if self.watched => locked(&self.done).map(|done| done.history),
            State::Aged | State::Stale => None,
        }
    }

    fn record(&self, dirty: u64, report: &RefreshReport) {
        *locked(&self.done) = Some(Done {
            at: Instant::now(),
            dirty,
            history: report.git_complete,
        });
        if report.changed + report.removed + report.code_changed + report.git_commits > 0 {
            self.touch();
        }
    }

    /// Runs `work` unless the index is known to be current. `work` is the whole refresh.
    pub(crate) fn through(
        self: &Arc<Self>,
        repo: &Repo,
        work: impl FnOnce() -> Result<RefreshReport, EngineError>,
    ) -> Result<RefreshReport, EngineError> {
        match self.state() {
            State::Fresh => return Ok(RefreshReport::default()),
            State::Aged if self.watched => {
                self.refresh_behind(repo);
                return Ok(RefreshReport::default());
            }
            State::Aged | State::Stale => {}
        }
        let running = Arc::clone(&self.running);
        let _one = locked(&running);
        // The refresh this read waited for may be the one it needed.
        if self.state() == State::Fresh {
            return Ok(RefreshReport::default());
        }
        let dirty = self.dirty.load(Ordering::SeqCst);
        let report = work()?;
        self.record(dirty, &report);
        Ok(report)
    }

    /// Starts a refresh on its own thread, unless one is already running.
    fn refresh_behind(self: &Arc<Self>, repo: &Repo) {
        if self.running.try_lock().is_err() {
            return;
        }
        let gate = Arc::clone(self);
        let repo = repo.clone();
        std::thread::spawn(move || {
            let running = Arc::clone(&gate.running);
            let Ok(_one) = running.try_lock() else { return };
            let dirty = gate.dirty.load(Ordering::SeqCst);
            // A failed refresh leaves the age as it was, so the next read tries again.
            if let Ok(report) = crate::refresh::refresh_unguarded(&repo) {
                gate.record(dirty, &report);
            }
        });
    }
}
