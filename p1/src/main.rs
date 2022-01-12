use std::env;
use std::thread;

fn is_prime(v: u32) -> bool {
    if v > 1 {
        let v_sqrt = (v as f64).sqrt() as u32;
        (2..v_sqrt).filter(|i| v % i == 0).count() == 0
    } else {
        false
    }
}

fn find_primes(start: u32, end: u32, step: u32) -> Vec<u32> {
    (start..=end)
        .step_by(step as usize)
        .filter(|v| is_prime(*v))
        .collect()
}

fn find_primes_in_range(start: u32, end: u32, threads: u32) -> Vec<u32> {
    let mut primes: Vec<u32> = (0..threads)
        .map(|i| thread::spawn(move || find_primes(start + i, end, threads)))
        .collect::<Vec<thread::JoinHandle<Vec<u32>>>>()
        .into_iter()
        .flat_map(|x| x.join().unwrap())
        .collect();
    primes.sort();
    primes
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

    let primes = find_primes_in_range(start, end, threads);
    println!("Primes: {:?}", primes);
}
