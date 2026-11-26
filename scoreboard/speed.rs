//! vt-mermaid timed in-process on every input of the board, alone on the machine, with what `render` returned and
//! whether it refuses the inputs mermaid refuses.

use std::collections::BTreeMap;
use std::error;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::Value;
use vt_mermaid::Error;

use crate::expected::Kind;

/// Inputs faster than this are timed again, keeping the fastest run, so noise does not hide the spread.
const RETRY_UNDER: Duration = Duration::from_millis(50);
const RUNS: usize = 5;

/// One kind timed at the wider width.
#[derive(Debug)]
pub(crate) struct Speed {
    pub(crate) kind: Kind,
    pub(crate) inputs: usize,
    pub(crate) median: Duration,
    pub(crate) p95: Duration,
    pub(crate) slowest: Duration,
    /// What `render` returned: `drawn`, `empty` for a canvas of no row, or the error's name, with how many inputs.
    pub(crate) outcomes: BTreeMap<String, usize>,
    pub(crate) mermaid_accepts: usize,
    /// Inputs `render` read whole: those over a size or count limit stop being read before mermaid's verdict can be
    /// matched.
    pub(crate) read_whole: usize,
    /// Of those, the inputs where `render` refuses as `Invalid` exactly when mermaid refuses.
    pub(crate) same_verdict: usize,
}

#[derive(Debug)]
struct Timed {
    elapsed: Duration,
    outcome: Outcome,
    mermaid_accepts: bool,
}

#[derive(Debug)]
struct Outcome {
    name: String,
    invalid: bool,
    read_whole: bool,
}

pub(crate) fn timed(root: &Path) -> Result<Vec<Speed>, Box<dyn error::Error>> {
    let verdicts = verdicts(root)?;
    Kind::ALL.into_iter().map(|kind| speed(root, kind, &verdicts)).collect()
}

/// The mermaid release the oracle ran.
pub(crate) fn mermaid_version(root: &Path) -> Result<String, Box<dyn error::Error>> {
    Ok(verdicts(root)?["mermaid"].as_str().ok_or("the oracle results name no mermaid version")?.to_owned())
}

fn verdicts(root: &Path) -> Result<Value, Box<dyn error::Error>> {
    Ok(serde_json::from_str(&fs::read_to_string(root.join("oracle/results/parse.json"))?)?)
}

fn speed(root: &Path, kind: Kind, verdicts: &Value) -> Result<Speed, Box<dyn error::Error>> {
    let mut paths: Vec<_> =
        fs::read_dir(root.join("corpus").join(kind.name()))?.map(|entry| entry.map(|entry| entry.path())).collect::<Result<_, _>>()?;
    paths.sort();
    let mut timings = Vec::new();
    for path in paths {
        let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let mermaid_accepts =
            verdicts["cases"][format!("{}/{name}", kind.name())]["accepted"].as_bool().ok_or("an input the oracle has no verdict on")?;
        timings.push(timed_once(&fs::read_to_string(&path)?, mermaid_accepts));
    }
    timings.sort_by_key(|timed| timed.elapsed);
    let at = |percent: usize| timings.get((timings.len().saturating_sub(1)) * percent / 100).map_or(Duration::ZERO, |timed| timed.elapsed);
    let mut outcomes = BTreeMap::new();
    for timed in &timings {
        *outcomes.entry(timed.outcome.name.clone()).or_default() += 1;
    }
    let read_whole: Vec<&Timed> = timings.iter().filter(|timed| timed.outcome.read_whole).collect();
    Ok(Speed {
        kind,
        inputs: timings.len(),
        median: at(50),
        p95: at(95),
        slowest: at(100),
        outcomes,
        mermaid_accepts: timings.iter().filter(|timed| timed.mermaid_accepts).count(),
        read_whole: read_whole.len(),
        same_verdict: read_whole.iter().filter(|timed| timed.mermaid_accepts != timed.outcome.invalid).count(),
    })
}

fn timed_once(source: &str, mermaid_accepts: bool) -> Timed {
    let (first, outcome) = once(source);
    let elapsed = if first < RETRY_UNDER { (1..RUNS).map(|_| once(source).0).fold(first, Duration::min) } else { first };
    Timed { elapsed, outcome, mermaid_accepts }
}

fn once(source: &str) -> (Duration, Outcome) {
    let start = Instant::now();
    let result = vt_mermaid::render(std::hint::black_box(source), crate::WIDTHS[1]);
    let elapsed = start.elapsed();
    (elapsed, Outcome::of(&result))
}

impl Outcome {
    fn of(result: &Result<vt_mermaid::Canvas<vt_mermaid::Role>, Error>) -> Self {
        match result {
            Ok(canvas) => {
                let name = if canvas.rows(vt_mermaid::GlyphSet::Unicode).len() == 0 { "empty" } else { "drawn" };
                Self { name: name.to_owned(), invalid: false, read_whole: true }
            }
            Err(error) => Self {
                name: format!("{error:?}").chars().take_while(char::is_ascii_alphanumeric).collect(),
                invalid: matches!(error, Error::Invalid { .. }),
                read_whole: !matches!(
                    error,
                    Error::TooLong { .. }
                        | Error::TooManyNodes { .. }
                        | Error::TooManyEdges { .. }
                        | Error::TooManyParticipants { .. }
                        | Error::TooManyMessages { .. }
                ),
            },
        }
    }
}
