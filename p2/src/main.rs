use std::collections::BinaryHeap;
use std::ops::{Add, Sub};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{self, Instant};

type WorkFunc = dyn FnOnce() -> () + Send;

struct Work {
    func: Box<WorkFunc>,
    when: time::Instant,
}
impl Work {
    fn until(&self) -> time::Duration {
        let now = Instant::now();
        if self.when > now {
            self.when.sub(Instant::now())
        } else {
            time::Duration::ZERO
        }
    }
}

struct WorkList {
    heap: BinaryHeap<Work>,
}
impl WorkList {
    fn new() -> WorkList {
        WorkList {
            heap: BinaryHeap::new(),
        }
    }

    fn push(&mut self, work: Work) {
        self.heap.push(work)
    }

    fn pop(&mut self) -> Option<Work> {
        self.heap.pop()
    }

    fn len(&self) -> usize {
        self.heap.len()
    }

    fn soonest(&self) -> time::Duration {
        match self.heap.peek() {
            Some(x) => x.until(),
            None => time::Duration::MAX,
        }
    }

    fn work_available(&self) -> bool {
        self.soonest() == time::Duration::ZERO
    }
}

struct SharedWorkerContext {
    work_list: WorkList,
    stop: bool,
}

struct WorkerContext {
    cvar: Condvar,
    shared: Mutex<SharedWorkerContext>,
}

struct Workers {
    threads: usize,
    ctx: Arc<WorkerContext>,
    handles: Vec<thread::JoinHandle<()>>,
}

impl Ord for Work {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.when.cmp(&self.when)
    }
}
impl PartialOrd for Work {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        return Some(self.cmp(other));
    }
}
impl PartialEq for Work {
    fn eq(&self, other: &Self) -> bool {
        self.when == other.when
    }
}
impl Eq for Work {}

impl Workers {
    pub fn new(threads: usize) -> Workers {
        let ctx = WorkerContext {
            cvar: Condvar::new(),
            shared: Mutex::new(SharedWorkerContext {
                work_list: WorkList::new(),
                stop: false,
            }),
        };
        let arc_ctx = Arc::new(ctx);
        Workers {
            threads,
            ctx: arc_ctx,
            handles: Vec::new(),
        }
    }

    pub fn start(&mut self) {
        let delta = self.threads - self.handles.len();
        if delta == 0 {
            panic!("no threads to create")
        }
        self.handles = (0..delta)
            .map(|_| self.ctx.clone())
            .map(|x| thread::spawn(move || Workers::thread_work(x)))
            .collect();
    }

    pub fn join(&mut self) {
        self.stop();
        (0..self.handles.len())
            .map(|_| self.handles.pop().unwrap())
            .for_each(|x| x.join().unwrap());
    }

    pub fn stop(&self) {
        let mut shared = self.ctx.shared.lock().unwrap();
        shared.stop = true;
        self.ctx.cvar.notify_all();
    }

    pub fn post<F>(&self, f: F)
    where
        F: FnOnce() -> () + Send + 'static,
    {
        self.post_timeout(f, time::Duration::ZERO)
    }

    pub fn post_timeout<F>(&self, f: F, timeout: time::Duration)
    where
        F: FnOnce() -> () + Send + 'static,
    {
        let work = Work {
            func: Box::new(f),
            when: Instant::now().add(timeout),
        };

        let mut shared = self.ctx.shared.lock().unwrap();
        shared.work_list.push(work);
        self.ctx.cvar.notify_one();
    }

    fn thread_work(ctx: Arc<WorkerContext>) {
        loop {
            let mut shared = ctx.shared.lock().unwrap();
            while !shared.work_list.work_available()
                && (!shared.stop || shared.stop && shared.work_list.len() > 0)
            {
                let duration = shared.work_list.soonest();
                shared = ctx.cvar.wait_timeout(shared, duration).unwrap().0
            }

            if shared.work_list.len() == 0 && shared.stop {
                return;
            }

            let work = shared.work_list.pop().unwrap();
            drop(shared);

            (work.func)();
        }
    }
}

fn main() {
    let mut workers = Workers::new(8);
    workers.start();

    println!("Scheduling...");
    for n in 0..5 {
        workers.post_timeout(
            move || println!("TIMEOUT: {} seconds", 1 + n),
            time::Duration::from_secs(1 + n),
        );
    }
    for n in 0..10 {
        workers.post(move || println!("INSTANT: {}", n))
    }
    for n in 5..10 {
        workers.post_timeout(
            move || println!("TIMEOUT: {} seconds", 1 + n),
            time::Duration::from_secs(1 + n),
        );
    }
    println!("Done Scheduling!");

    println!("Waiting for workers...");
    workers.join();
    println!("Done waiting!");
}
