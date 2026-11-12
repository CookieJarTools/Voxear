//! Work a terrain does on threads of its own, so the frame does not wait
//! for it: background work that is found done a frame or two later
//! ([`spawn`]), and a long piece of work shared out over the threads while
//! the caller waits ([`each`]). How many threads run terrain work at once is
//! a setting a game can change as it runs ([`set_threads`], from
//! `WorldBudget::threads`).

use std::sync::{
    Arc, Condvar, Mutex, OnceLock,
    atomic::{AtomicUsize, Ordering},
    mpsc,
};

type Job = Box<dyn FnOnce() + Send>;

/// How many cores there are to share work over.
fn cores() -> usize {
    std::thread::available_parallelism().map_or(4, |n| n.get())
}

/// Background threads taking jobs from one queue in the order they came, no
/// more of them at work at once than their limit.
struct Pool {
    send: Mutex<mpsc::Sender<Job>>,
    started: usize,
    gate: Arc<Gate>,
}

/// How many of a pool's threads are at work, and how many may be.
struct Gate {
    working: Mutex<usize>,
    limit: AtomicUsize,
    freed: Condvar,
}

impl Gate {
    /// Waits until fewer than the limit are at work, then counts one more.
    fn enter(&self) {
        let mut working = self.working.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        while *working >= self.limit.load(Ordering::Acquire).max(1) {
            working = self.freed.wait(working).unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        *working += 1;
    }

    fn leave(&self) {
        *self.working.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) -= 1;
        self.freed.notify_all();
    }

    fn set_limit(&self, limit: usize) {
        self.limit.store(limit.max(1), Ordering::Release);
        // Taken under the lock, so none waiting misses that it may go on.
        drop(self.working.lock());
        self.freed.notify_all();
    }
}

impl Pool {
    /// `threads` background threads named after `name`, all of them at work
    /// at once until a limit is set.
    fn new(threads: usize, name: &str) -> Self {
        let (send, receive) = mpsc::channel::<Job>();
        let receive = Arc::new(Mutex::new(receive));
        let gate = Arc::new(Gate { working: Mutex::new(0), limit: AtomicUsize::new(threads), freed: Condvar::new() });
        let mut started = 0;
        for at in 0..threads {
            let (receive, gate) = (receive.clone(), gate.clone());
            let thread = std::thread::Builder::new().name(format!("{name} {at}")).spawn(move || {
                voxear_core::profiling::register_thread("Terrain");
                loop {
                    let job = match receive.lock() {
                        Ok(queue) => queue.recv(),
                        Err(_) => return,
                    };
                    let Ok(job) = job else { return };
                    // Its turn to work, once fewer than the limit are: a thread waiting for a job holds none.
                    gate.enter();
                    job();
                    gate.leave();
                }
            });
            started += usize::from(thread.is_ok());
        }
        Self { send: Mutex::new(send), started, gate }
    }

    fn spawn(&self, job: Job) {
        if self.started == 0 {
            job();
            return;
        }
        if let Err(mpsc::SendError(job)) = self.send.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).send(job) {
            job();
        }
    }

    /// How many of its threads may work at once.
    fn working_at_most(&self) -> usize {
        self.gate.limit.load(Ordering::Acquire).clamp(1, self.started.max(1))
    }
}

/// The terrain's background threads: one fewer than the cores, so the
/// frame keeps one to itself.
fn pool() -> &'static Pool {
    static POOL: OnceLock<Pool> = OnceLock::new();
    POOL.get_or_init(|| Pool::new(cores().saturating_sub(1).max(1), "terrain"))
}

/// Lets `threads` of the terrain's threads work at once from now on, at
/// least one, and no more than were started (one fewer than the cores):
/// work already started finishes, and none starts past the new number.
pub fn set_threads(threads: usize) {
    pool().gate.set_limit(threads);
}

/// How many threads terrain work runs on at once now.
pub fn threads() -> usize {
    pool().working_at_most()
}

/// Does `job` on a background thread, as soon as one is free: in the order
/// jobs came. Where no background thread could be started, at once.
pub fn spawn(job: impl FnOnce() + Send + 'static) {
    pool().spawn(Box::new(job));
}

/// `work` done on each of `items`, shared out over as many threads as
/// terrain work may use while the caller waits: what each gave, in the
/// items' order.
pub fn each<T: Sync, R: Send>(items: &[T], work: impl Fn(&T) -> R + Sync) -> Vec<R> {
    if items.len() <= 1 {
        return items.iter().map(work).collect();
    }
    let next = AtomicUsize::new(0);
    let made = Mutex::new(Vec::with_capacity(items.len()));
    // The caller's own thread is one of them.
    let threads = (threads() + 1).min(cores()).min(items.len());
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    let at = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(at) else { break };
                    let done = work(item);
                    made.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push((at, done));
                }
            });
        }
    });
    let mut made = made.into_inner().unwrap_or_else(|poisoned| poisoned.into_inner());
    made.sort_unstable_by_key(|(at, _)| *at);
    made.into_iter().map(|(_, done)| done).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_shared_out_comes_back_in_order_and_background_work_gets_done() {
        let items: Vec<u32> = (0..100).collect();
        assert_eq!(each(&items, |n| n * 2), (0..100).map(|n| n * 2).collect::<Vec<_>>());
        let (send, receive) = mpsc::channel();
        for n in 0..8 {
            let send = send.clone();
            spawn(move || send.send(n).expect("the test waits"));
        }
        let mut done: Vec<u32> = (0..8).map(|_| receive.recv().expect("each job")).collect();
        done.sort_unstable();
        assert_eq!(done, (0..8).collect::<Vec<_>>());
    }

    #[test]
    fn a_pool_works_no_more_jobs_at_once_than_its_limit_which_can_change_as_it_runs() {
        use std::time::Duration;
        let pool = Pool::new(6, "test");
        pool.gate.set_limit(2);
        assert_eq!(pool.working_at_most(), 2);
        // Each job counts itself in while it works, and notes the most at once.
        let (now, most) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let run = |pool: &Pool, jobs: usize| {
            let (send, receive) = mpsc::channel();
            for _ in 0..jobs {
                let (now, most, send) = (now.clone(), most.clone(), send.clone());
                pool.spawn(Box::new(move || {
                    let at = now.fetch_add(1, Ordering::AcqRel) + 1;
                    most.fetch_max(at, Ordering::AcqRel);
                    std::thread::sleep(Duration::from_millis(5));
                    now.fetch_sub(1, Ordering::AcqRel);
                    send.send(()).expect("the test waits");
                }));
            }
            for _ in 0..jobs {
                receive.recv_timeout(Duration::from_secs(30)).expect("every job done");
            }
        };
        run(&pool, 24);
        assert_eq!(most.load(Ordering::Acquire), 2, "two at once at most, and two did work together");
        // Raised as it runs: more at once, never more than were started.
        most.store(0, Ordering::Release);
        pool.gate.set_limit(64);
        assert_eq!(pool.working_at_most(), 6);
        run(&pool, 48);
        let most = most.load(Ordering::Acquire);
        assert!(most > 2 && most <= 6, "{most} at once");
    }
}
