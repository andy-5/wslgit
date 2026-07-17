use std::env;
use std::hint::black_box;
use std::process::Command;
use std::time::{Duration, Instant};

const WSLGIT: &str = env!("CARGO_BIN_EXE_wslgit");
const WARMUP_ITERATIONS: usize = 2;
const DEFAULT_ITERATIONS: usize = 10;

fn run(args: &[&str]) {
    let output = Command::new(WSLGIT)
        .args(args)
        .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
        .output()
        .unwrap_or_else(|error| panic!("failed to start benchmark command: {}", error));

    assert!(
        output.status.success(),
        "benchmark command exited with {}",
        output.status
    );
    assert!(
        !output.stdout.is_empty(),
        "benchmark command produced no output"
    );
    black_box(output);
}

fn benchmark(name: &str, iterations: usize, args: &[&str]) {
    for _ in 0..WARMUP_ITERATIONS {
        run(args);
    }

    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        run(args);
        samples.push(start.elapsed());
    }

    let total: Duration = samples.iter().sum();
    let mean = total.as_secs_f64() * 1_000.0 / iterations as f64;
    let min = samples.iter().min().unwrap().as_secs_f64() * 1_000.0;
    let max = samples.iter().max().unwrap().as_secs_f64() * 1_000.0;
    println!(
        "{}: mean {:.2} ms (min {:.2} ms, max {:.2} ms, n={})",
        name, mean, min, max, iterations
    );
}

fn benchmark_iterations() -> usize {
    let mut args = env::args().skip(1);
    let mut configured_iterations = None;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--bench" => {}
            "--iterations" => {
                assert!(
                    configured_iterations.is_none(),
                    "--iterations may only be supplied once"
                );
                configured_iterations = Some(
                    args.next()
                        .expect("--iterations requires a positive integer")
                        .parse::<usize>()
                        .expect("--iterations requires a positive integer"),
                );
            }
            _ => panic!("unexpected benchmark argument: {}", argument),
        }
    }

    let iterations = configured_iterations.unwrap_or(DEFAULT_ITERATIONS);
    assert!(iterations > 0, "--iterations must be greater than zero");
    iterations
}

fn main() {
    let iterations = benchmark_iterations();

    let absolute_path = env::current_dir()
        .expect("failed to resolve benchmark directory")
        .join("src\\main.rs")
        .to_string_lossy()
        .into_owned();

    benchmark("no_translation", iterations, &["--version"]);
    benchmark(
        "translate_absolute_argument",
        iterations,
        &["log", "-n1", "--oneline", "--", &absolute_path],
    );
    benchmark(
        "translate_relative_argument",
        iterations,
        &["log", "-n1", "--oneline", "--", "src\\main.rs"],
    );
    benchmark(
        "translate_output",
        iterations,
        &["rev-parse", "--show-toplevel"],
    );
}
