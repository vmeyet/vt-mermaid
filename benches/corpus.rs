//! Times `render` on every flowchart, sequence, state, class and ER diagram of the corpus and prints the spread of each kind:
//! `cargo bench --bench corpus`.
//!
//! divan times one input at a time; this sweep answers how long a diagram takes across real inputs, and which one
//! takes longest.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The width mrk usually gets, so the top-down retry runs as it does there.
const WIDTH: u16 = 100;
/// Inputs faster than this are timed again, keeping the fastest run, so noise does not hide the spread.
const RETRY_UNDER: Duration = Duration::from_millis(50);
const RUNS: usize = 5;

/// `cargo test --all-targets` runs this without `--bench`, and the sweep takes minutes.
fn main() -> std::io::Result<()> {
    if !std::env::args().any(|arg| arg == "--bench") {
        return Ok(());
    }
    for (kind, plural) in [
        ("flowchart", "flowcharts"),
        ("sequence", "sequence diagrams"),
        ("state", "state diagrams"),
        ("class", "class diagrams"),
        ("er", "ER diagrams"),
    ] {
        let mut timings: Vec<(Duration, PathBuf, &'static str)> =
            diagrams(kind)?.into_iter().map(|(path, source)| timed(&source, path)).collect();
        timings.sort_by_key(|&(elapsed, ..)| elapsed);
        print_spread(&timings, plural);
    }
    Ok(())
}

fn diagrams(kind: &str) -> std::io::Result<Vec<(PathBuf, String)>> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus").join(kind);
    let mut paths: Vec<PathBuf> = fs::read_dir(folder)?.map(|entry| entry.map(|entry| entry.path())).collect::<Result<_, _>>()?;
    paths.sort();
    paths.into_iter().map(|path| fs::read_to_string(&path).map(|source| (path, source))).collect()
}

fn timed(source: &str, path: PathBuf) -> (Duration, PathBuf, &'static str) {
    let (first, outcome) = once(source);
    let elapsed = if first < RETRY_UNDER { (1..RUNS).map(|_| once(source).0).fold(first, Duration::min) } else { first };
    (elapsed, path, outcome)
}

fn once(source: &str) -> (Duration, &'static str) {
    let start = Instant::now();
    let result = vt_mermaid::render(std::hint::black_box(source), WIDTH);
    let elapsed = start.elapsed();
    let outcome = match result {
        Ok(_) => "drawn",
        Err(vt_mermaid::Error::TooWide { .. }) => "too wide",
        Err(vt_mermaid::Error::TooLong { .. } | vt_mermaid::Error::TooLarge { .. } | vt_mermaid::Error::TooComplex) => "over a size limit",
        Err(
            vt_mermaid::Error::TooManyNodes { .. }
            | vt_mermaid::Error::TooManyEdges { .. }
            | vt_mermaid::Error::TooManyParticipants { .. }
            | vt_mermaid::Error::TooManyMessages { .. },
        ) => "over a count limit",
        Err(_) => "refused",
    };
    (elapsed, outcome)
}

fn print_spread(timings: &[(Duration, PathBuf, &str)], plural: &str) {
    let at = |percent: usize| timings[(timings.len() - 1) * percent / 100].0;
    println!("{} {plural} at {WIDTH} columns", timings.len());
    println!("median {:?}, p95 {:?}, p99 {:?}", at(50), at(95), at(99));
    println!("slowest:");
    for (elapsed, path, outcome) in timings.iter().rev().take(5) {
        let name = path.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        println!("  {elapsed:>12.1?}  {outcome:<18}  {name}");
    }
}
