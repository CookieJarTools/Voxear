//! Work a terrain does on threads of its own, so the frame does not wait
//! for it: background work that is found done a frame or two later
//! ([`spawn`]), and a long piece of work shared out over the threads while
//! the caller waits ([`each`]). How many threads run terrain work at once is
//! a setting a game can change as it runs ([`set_threads`], from
//! `WorldBudget::threads`).

use std::{
    collections::VecDeque,
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
};

type Job = Box<dyn FnOnce() + Send>;

/// How many cores there are to share work over.
fn cores() -> usize {
    std::thread::available_parallelism().map_or(4, |n| n.get())
}

/// Background threads taking jobs from one queue in the order they came, no
/// more of them at work at once than their limit; and quick jobs, a moment
/// each, from a queue of their own taken first, past the limit.
struct Pool {
    queue: Arc<Queue>,
    started: usize,
    gate: Arc<Gate>,
}

/// The jobs waiting: quick ones, and the rest.
#[derive(Default)]
struct Queue {
    jobs: Mutex<(VecDeque<Job>, VecDeque<Job>)>,
    came: Condvar,
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
        let queue = Arc::new(Queue::default());
        let gate = Arc::new(Gate { working: Mutex::new(0), limit: AtomicUsize::new(threads), freed: Condvar::new() });
        let mut started = 0;
        for at in 0..threads {
            let (queue, gate) = (queue.clone(), gate.clone());
            let thread = std::thread::Builder::new().name(format!("{name} {at}")).spawn(move || {
                voxear_core::profiling::register_thread("Terrain");
                lower_priority();
                loop {
                    let (job, quick) = {
                        let mut jobs = queue.jobs.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                        loop {
                            if let Some(job) = jobs.0.pop_front() {
                                break (job, true);
                            }
                            if let Some(job) = jobs.1.pop_front() {
                                break (job, false);
                            }
                            jobs = queue.came.wait(jobs).unwrap_or_else(|poisoned| poisoned.into_inner());
                        }
                    };
                    // A quick job at once; the rest in its turn, once fewer than the limit are at work: a thread
                    // waiting for a job holds none.
                    if quick {
                        job();
                        continue;
                    }
                    gate.enter();
                    job();
                    gate.leave();
                }
            });
            started += usize::from(thread.is_ok());
        }
        Self { queue, started, gate }
    }

    fn spawn(&self, job: Job, quick: bool) {
        if self.started == 0 {
            job();
            return;
        }
        let mut jobs = self.queue.jobs.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        match quick {
            true => jobs.0.push_back(job),
            false => jobs.1.push_back(job),
        }
        drop(jobs);
        self.queue.came.notify_one();
    }

    /// How many of its threads may work at once.
    fn working_at_most(&self) -> usize {
        self.gate.limit.load(Ordering::Acquire).clamp(1, self.started.max(1))
    }
}

/// Lets the thread it is called on give way to the game's own: a terrain
/// thread runs a little behind the frame, the sound and the renderer's
/// threads, so making terrain on most of a machine's cores never holds a
/// frame back. Where the system cannot, it runs as it was.
pub fn lower_priority() {
    #[cfg(unix)]
    // SAFETY: setpriority with this thread's own id only changes how its own thread is scheduled.
    unsafe {
        let thread = libc::syscall(libc::SYS_gettid) as libc::id_t;
        libc::setpriority(libc::PRIO_PROCESS, thread, 5);
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
    pool().spawn(Box::new(job), false);
}

/// Does `job`, a moment's work such as reading what was kept, on a
/// background thread before any job waiting, and whether or not as many as
/// may are at work: so it never waits behind long ones.
pub fn spawn_quick(job: impl FnOnce() + Send + 'static) {
    pool().spawn(Box::new(job), true);
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
    use std::sync::mpsc;

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
        // Each job counts itself in while it works, and notes the most at once. It works until `meet` are at work
        // together, or two seconds pass, so they meet however busy the machine is with other work.
        let (now, most) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let run = |pool: &Pool, jobs: usize, meet: usize| {
            let (send, receive) = mpsc::channel();
            for _ in 0..jobs {
                let (now, most, send) = (now.clone(), most.clone(), send.clone());
                pool.spawn(
                    Box::new(move || {
                        let at = now.fetch_add(1, Ordering::AcqRel) + 1;
                        most.fetch_max(at, Ordering::AcqRel);
                        let start = std::time::Instant::now();
                        while now.load(Ordering::Acquire) < meet && start.elapsed() < Duration::from_secs(2) {
                            std::thread::sleep(Duration::from_millis(1));
                        }
                        most.fetch_max(now.load(Ordering::Acquire), Ordering::AcqRel);
                        now.fetch_sub(1, Ordering::AcqRel);
                        send.send(()).expect("the test waits");
                    }),
                    false,
                );
            }
            for _ in 0..jobs {
                receive.recv_timeout(Duration::from_secs(30)).expect("every job done");
            }
        };
        run(&pool, 24, 2);
        assert_eq!(most.load(Ordering::Acquire), 2, "two at once at most, and two did work together");
        // Raised as it runs: more at once, never more than were started.
        most.store(0, Ordering::Release);
        pool.gate.set_limit(64);
        assert_eq!(pool.working_at_most(), 6);
        run(&pool, 48, 3);
        let most = most.load(Ordering::Acquire);
        assert!(most > 2 && most <= 6, "{most} at once");
    }

    #[test]
    fn a_quick_job_runs_at_once_while_every_thread_allowed_is_busy_with_long_ones() {
        use std::time::{Duration, Instant};
        let pool = Pool::new(4, "quick test");
        pool.gate.set_limit(1);
        let (long, quick) = (mpsc::channel(), mpsc::channel());
        // Long jobs, a second each, more than may work at once.
        for _ in 0..3 {
            let send = long.0.clone();
            pool.spawn(
                Box::new(move || {
                    std::thread::sleep(Duration::from_secs(1));
                    let _ = send.send(());
                }),
                false,
            );
        }
        std::thread::sleep(Duration::from_millis(50));
        let start = Instant::now();
        let send = quick.0.clone();
        pool.spawn(Box::new(move || send.send(start.elapsed()).expect("the test waits")), true);
        let waited = quick.1.recv_timeout(Duration::from_secs(10)).expect("the quick job done");
        assert!(waited < Duration::from_millis(500), "it waited {waited:?} behind the long ones");
        for _ in 0..3 {
            long.1.recv_timeout(Duration::from_secs(10)).expect("each long job done");
        }
    }
}
