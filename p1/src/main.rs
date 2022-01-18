use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::env;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, RecvError, Sender};
use std::thread;

fn is_prime(v: u32) -> bool {
    if v > 1 {
        let v_sqrt = (v as f64).sqrt() as u32 + 1;
        !(2..v_sqrt).any(|x| v % x == 0)
    } else {
        false
    }
}

fn find_primes(start: u32, end: u32, step: u32, tx: Sender<u32>) {
    (start..=end)
        .step_by(step as usize)
        .filter(|v| is_prime(*v))
        .for_each(|v| tx.send(v).unwrap());
    drop(tx);
}

fn find_primes_in_range(start: u32, end: u32, threads: u32) -> BinaryHeap<Reverse<u32>> {
    let (tx, rx): (Sender<u32>, Receiver<u32>) = mpsc::channel();
    let handles: Vec<thread::JoinHandle<()>> = (0..threads)
        .map(|x| (x, tx.clone()))
        .map(|x| thread::spawn(move || find_primes(start + x.0, end, threads, x.1)))
        .collect();
    drop(tx);
    let mut heap = BinaryHeap::new();

    loop {
        let result = rx.recv();
        if result == Err(RecvError) {
            break;
        }
        heap.push(Reverse(result.unwrap()));
    }
    handles.into_iter().for_each(|x| x.join().unwrap());
    heap
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let start = args[1].parse().unwrap();
    let end = args[2].parse().unwrap();
    let threads = args[3].parse().unwrap();

    println!(
        "Finding prime numbers in range [{}, {}] with {} threads...",
        start, end, threads
    );

    if threads <= 0 {
        panic!("Cannot use {} threads.", threads)
    }

    let mut primes = find_primes_in_range(start, end, threads);
    print!("Primes: ");
    while primes.len() > 0 {
        let Reverse(p) = primes.pop().unwrap();
        print!("{} ", p);
    }
    println!("");
}
