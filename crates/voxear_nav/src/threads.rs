//! Navigation's own threads: a few, apart from the terrain's, so a search
//! asked for this frame is not kept waiting behind ground being made.
//!
//! Work is a [`Job`]: started on a thread, and taken when it is wanted.
//! Taken before a thread has begun it, it is done there and then by the
//! taker; taken while a thread is at it, the taker waits for it. So what is
//! asked for one frame is always there the next, whatever the threads were
//! doing.

use std::sync::{Arc, Condvar, Mutex, OnceLock, mpsc};

type Work = Box<dyn FnOnce() + Send>;

/// How many threads navigation keeps: a quarter of the cores, one at least
/// and three at most.
fn count() -> usize {
    let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
    (cores / 4).clamp(1, 3)
}

fn queue() -> &'static Mutex<mpsc::Sender<Work>> {
    static QUEUE: OnceLock<Mutex<mpsc::Sender<Work>>> = OnceLock::new();
    QUEUE.get_or_init(|| {
        let (send, receive) = mpsc::channel::<Work>();
        let receive = Arc::new(Mutex::new(receive));
        for at in 0..count() {
            let receive = receive.clone();
            let _ = std::thread::Builder::new().name(format!("navigation {at}")).spawn(move || {
                voxear_core::profiling::register_thread("Navigation");
                loop {
                    let work = match receive.lock() {
                        Ok(queue) => queue.recv(),
                        Err(_) => return,
                    };
                    let Ok(work) = work else { return };
                    work();
                }
            });
        }
        Mutex::new(send)
    })
}

/// Does `work` on one of navigation's threads, in the order work came; at
/// once, here, where there are no threads to do it.
pub fn spawn(work: impl FnOnce() + Send + 'static) {
    let sent = queue().lock().unwrap_or_else(|poisoned| poisoned.into_inner()).send(Box::new(work));
    if let Err(mpsc::SendError(work)) = sent {
        work();
    }
}

enum State<T> {
    Waiting(Box<dyn FnOnce() -> T + Send>),
    Running,
    Done(T),
    Taken,
}

/// Work started on navigation's threads, whose result is taken later.
pub struct Job<T> {
    state: Mutex<State<T>>,
    done: Condvar,
}

impl<T: Send + 'static> Job<T> {
    /// Starts `work` on navigation's threads.
    pub fn start(work: impl FnOnce() -> T + Send + 'static) -> Arc<Self> {
        let job = Arc::new(Self { state: Mutex::new(State::Waiting(Box::new(work))), done: Condvar::new() });
        let run = job.clone();
        spawn(move || run.run());
        job
    }

    /// `work`, done at once, as a job already finished.
    pub fn done(result: T) -> Arc<Self> {
        Arc::new(Self { state: Mutex::new(State::Done(result)), done: Condvar::new() })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State<T>> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Does the work, unless the taker took it to do itself first.
    fn run(&self) {
        let work = {
            let mut state = self.lock();
            match std::mem::replace(&mut *state, State::Running) {
                State::Waiting(work) => work,
                other => {
                    *state = other;
                    return;
                }
            }
        };
        let result = work();
        *self.lock() = State::Done(result);
        self.done.notify_all();
    }

    /// Whether the result is there to take.
    pub fn is_done(&self) -> bool {
        matches!(*self.lock(), State::Done(_))
    }

    /// Whether a thread has begun it, or it is done.
    pub fn is_begun(&self) -> bool {
        !matches!(*self.lock(), State::Waiting(_))
    }

    /// The result, if it is done; `None` while it is not, or once taken.
    pub fn try_take(&self) -> Option<T> {
        let mut state = self.lock();
        match std::mem::replace(&mut *state, State::Taken) {
            State::Done(result) => Some(result),
            other => {
                *state = other;
                None
            }
        }
    }

    /// The result: done here and now if no thread has begun it, waited for
    /// if one has. `None` once taken.
    pub fn take(&self) -> Option<T> {
        let mut state = self.lock();
        loop {
            match std::mem::replace(&mut *state, State::Taken) {
                State::Done(result) => return Some(result),
                // Taken to do here: no thread does it now, and nobody waits for it.
                State::Waiting(work) => {
                    drop(state);
                    return Some(work());
                }
                State::Running => {
                    *state = State::Running;
                    state = self.done.wait(state).unwrap_or_else(|poisoned| poisoned.into_inner());
                }
                State::Taken => return None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_job_is_done_on_a_thread_or_by_whoever_takes_it_first() {
        let job = Job::start(|| 6 * 7);
        assert_eq!(job.take(), Some(42));
        assert_eq!(job.take(), None, "taken once");
        let many: Vec<_> = (0..64).map(|n| Job::start(move || n * 2)).collect();
        let sum: i32 = many.iter().map(|job| job.take().unwrap()).sum();
        assert_eq!(sum, (0..64).map(|n| n * 2).sum());
        assert_eq!(Job::done(5).try_take(), Some(5));
    }
}
