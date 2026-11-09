//! Work a terrain does on threads of its own, so the frame does not wait
//! for it: background work that is found done a frame or two later
//! ([`spawn`]), and a long piece of work shared out over every core while
//! the caller waits ([`each`]).

use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicUsize, Ordering},
    mpsc,
};

type Job = Box<dyn FnOnce() + Send>;

/// How many cores there are to share work over.
fn cores() -> usize {
    std::thread::available_parallelism().map_or(4, |n| n.get())
}

/// The background threads' queue, and how many of them started: one
/// fewer than the cores, so the frame keeps one to itself.
fn queue() -> &'static (Mutex<mpsc::Sender<Job>>, usize) {
    static QUEUE: OnceLock<(Mutex<mpsc::Sender<Job>>, usize)> = OnceLock::new();
    QUEUE.get_or_init(|| {
        let (send, receive) = mpsc::channel::<Job>();
        let receive = Arc::new(Mutex::new(receive));
        let mut started = 0;
        for at in 0..cores().saturating_sub(1).max(1) {
            let receive = receive.clone();
            let thread = std::thread::Builder::new().name(format!("terrain {at}")).spawn(move || {
                voxear_core::profiling::register_thread("Terrain");
                loop {
                    // One thread waits on the queue at a time; the rest wait their turn for it.
                    let job = match receive.lock() {
                        Ok(queue) => queue.recv(),
                        Err(_) => return,
                    };
                    match job {
                        Ok(job) => job(),
                        Err(_) => return,
                    }
                }
            });
            started += usize::from(thread.is_ok());
        }
        (Mutex::new(send), started)
    })
}

/// Does `job` on a background thread, as soon as one is free: in the order
/// jobs came. Where no background thread could be started, at once.
pub fn spawn(job: impl FnOnce() + Send + 'static) {
    let (send, started) = queue();
    if *started == 0 {
        job();
        return;
    }
    let job: Job = Box::new(job);
    if let Err(mpsc::SendError(job)) = send.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).send(job) {
        job();
    }
}

/// `work` done on each of `items`, shared out over every core while the
/// caller waits: what each gave, in the items' order.
pub fn each<T: Sync, R: Send>(items: &[T], work: impl Fn(&T) -> R + Sync) -> Vec<R> {
    if items.len() <= 1 {
        return items.iter().map(work).collect();
    }
    let next = AtomicUsize::new(0);
    let made = Mutex::new(Vec::with_capacity(items.len()));
    std::thread::scope(|scope| {
        for _ in 0..cores().min(items.len()) {
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
}
