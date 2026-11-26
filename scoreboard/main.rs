//! vt-mermaid's drawings measured against other text renderers: `cargo run --release --example scoreboard`.
//!
//! Options: `--kind sequence`, `state`, `class` or `er` measures that kind instead of flowcharts, `--sample <n>` runs one input in n,
//! `--tools <a,b>` runs only those tools and keeps the others' last results, `--gallery <file.html>` writes the gallery
//! there instead of `target/scoreboard/gallery.html` (`gallery-sequence.html` for sequence diagrams),
//! `--markdown <file.md>` measures every kind and writes the comparison page there.
//! [`README.md`](README.md) defines the metrics; `install.sh` installs the competitors.

mod expected;
mod gallery;
mod lines;
mod machine;
mod metrics;
mod others;
mod page;
mod results;
mod speed;
mod tool;

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

use expected::{Expected, Kind};
use metrics::{Exact, Metrics};
use page::{Page, Sample};
use results::{Board, Counts, InputResult, Times};
use tool::{Run, Status, Tool};

const WIDTHS: [u16; 2] = [80, 100];

#[derive(Debug)]
struct Options {
    kind: Kind,
    sample: Option<usize>,
    tools: Vec<Tool>,
    gallery: Option<PathBuf>,
    markdown: Option<PathBuf>,
}

#[derive(Debug)]
struct Input {
    path: String,
    source: String,
}

/// One tool on one input at one width.
#[derive(Debug, Clone, Copy)]
struct Job {
    input: usize,
    tool: Tool,
    width: u16,
}

#[derive(Debug)]
struct Done {
    job: Job,
    run: Run,
    exact: Option<Exact>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["--render", name, width] => tool::render_in_process(name, width.parse()?),
        ["--measure", width] => measure_in_process(width.parse()?),
        _ => {
            let options = Options::parse(&args)?;
            match &options.markdown {
                Some(page) => comparison(&options, page),
                None => scoreboard(&options),
            }
        }
    }
}

fn scoreboard(options: &Options) -> Result<(), Box<dyn Error>> {
    let gallery = options.gallery.clone().unwrap_or_else(|| default_gallery(options.kind));
    let board = board(options.kind, options, &gallery)?;
    println!("{}\ngallery: {}", board.summary, gallery.display());
    Ok(())
}

/// Every kind on the board, then the kinds only other tools draw, vt-mermaid timed alone, and the page written from them.
fn comparison(options: &Options, path: &Path) -> Result<(), Box<dyn Error>> {
    let started = Instant::now();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let boards = Kind::ALL.into_iter().map(|kind| board(kind, options, &default_gallery(kind))).collect::<Result<Vec<_>, _>>()?;
    let installed = installed(&options.tools);
    let others = others::drawn(&root.join("corpus"), &installed)?;
    let speeds = speed::timed(root)?;
    let samples = samples(root, &boards)?;
    let weak_spots = page::WEAK_SPOTS.iter().map(|spot| Tool::VtMermaid.run(spot.source, WIDTHS[0])).collect::<io::Result<_>>()?;
    let mermaid = speed::mermaid_version(root)?;
    let minutes = started.elapsed().as_secs().div_ceil(60);
    let page = Page { boards, installed, others, speeds, samples, weak_spots, machine: machine::describe(), mermaid, minutes };
    fs::write(path, page::markdown(&page))?;
    println!("wrote {}", path.display());
    Ok(())
}

/// One kind measured: every tool on every input, the results and summary written, the gallery too.
fn board(kind: Kind, options: &Options, gallery: &Path) -> Result<Board, Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let inputs = inputs(&root.join("corpus").join(kind.name()), kind, options.sample)?;
    let expected = expected::read(&root.join(format!("oracle/results/{}.json", kind.name())), kind)?;
    let tools = installed(&options.tools);
    let done = run_all(&inputs, &tools, kind)?;
    let results_folder = root.join("scoreboard/results").join(kind.results());
    let folder = results_folder.join("inputs");
    fs::create_dir_all(&folder)?;
    let results = inputs
        .iter()
        .enumerate()
        .map(|(index, input)| {
            let expected = expected.get(&input.path);
            InputResult::merged(&folder, &input.path, expected.map(Counts::of), fresh_metrics(&done, index, expected, kind))
        })
        .collect::<io::Result<Vec<_>>>()?;
    for result in &results {
        result.write(&folder)?;
    }
    let times = times(&done);
    let summary = results::summary(&results, &WIDTHS, &times, kind, options.sample);
    fs::write(results_folder.join("summary.md"), &summary)?;
    write_gallery(gallery, &inputs, &results, &done)?;
    Ok(Board { kind, results, times, summary })
}

/// The opening example of a syntax page in mermaid's docs, drawn by vt-mermaid and by the best other tool on its kind.
fn samples(root: &Path, boards: &[Board]) -> Result<Vec<Sample>, Box<dyn Error>> {
    page::SAMPLES
        .iter()
        .map(|&(kind, input)| {
            let source = fs::read_to_string(root.join("corpus").join(input))?;
            let rival = boards.iter().find(|board| board.kind == kind).and_then(|board| board.best_rival(WIDTHS[0]));
            let ours = drawn(Tool::VtMermaid, &source)?;
            let rival = match rival {
                Some(tool) => Some((tool, drawn(tool, &source)?)),
                None => None,
            };
            Ok(Sample { input, source, ours, rival })
        })
        .collect()
}

/// A tool's drawing at the narrower width, none when the tool is not installed here.
fn drawn(tool: Tool, source: &str) -> io::Result<Option<Run>> {
    if tool.is_installed() { tool.run(source, WIDTHS[0]).map(Some) } else { Ok(None) }
}

impl Options {
    fn parse(args: &[String]) -> Result<Self, Box<dyn Error>> {
        let mut options = Self { kind: Kind::Flowchart, sample: None, tools: Tool::ALL.to_vec(), gallery: None, markdown: None };
        let mut args = args.iter();
        while let Some(flag) = args.next() {
            let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
            match flag.as_str() {
                "--sample" => options.sample = Some(value.parse::<usize>()?.max(1)),
                "--tools" => {
                    options.tools = value
                        .split(',')
                        .map(|name| Tool::named(name).ok_or_else(|| format!("no tool named {name}")))
                        .collect::<Result<_, _>>()?;
                }
                "--gallery" => options.gallery = Some(PathBuf::from(value)),
                "--kind" => options.kind = Kind::named(value).ok_or_else(|| format!("no diagram kind named {value}"))?,
                "--markdown" => options.markdown = Some(PathBuf::from(value)),
                _ => return Err(format!("unknown option {flag}").into()),
            }
        }
        if options.markdown.is_some() && (options.sample.is_some() || options.gallery.is_some() || options.kind != Kind::Flowchart) {
            return Err("--markdown measures every input of every kind: drop --sample, --kind and --gallery".into());
        }
        Ok(options)
    }
}

fn default_gallery(kind: Kind) -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target"), PathBuf::from);
    let name = match kind {
        Kind::Flowchart => "gallery.html".to_owned(),
        Kind::Sequence | Kind::State | Kind::Class | Kind::Er => format!("gallery-{}.html", kind.name()),
    };
    target.join("scoreboard").join(name)
}

/// The diagrams of one kind in the corpus in name order, or one in `sample` of them.
fn inputs(folder: &Path, kind: Kind, sample: Option<usize>) -> io::Result<Vec<Input>> {
    let mut paths: Vec<PathBuf> = fs::read_dir(folder)?.map(|entry| entry.map(|entry| entry.path())).collect::<io::Result<_>>()?;
    paths.sort();
    paths
        .into_iter()
        .step_by(sample.unwrap_or(1))
        .map(|path| {
            let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            Ok(Input { source: fs::read_to_string(&path)?, path: format!("{}/{name}", kind.name()) })
        })
        .collect()
}

fn installed(tools: &[Tool]) -> Vec<Tool> {
    let (present, missing): (Vec<Tool>, Vec<Tool>) = tools.iter().partition(|tool| tool.is_installed());
    for tool in missing {
        eprintln!("{} is not installed: skipped, its last results are kept (run scoreboard/install.sh)", tool.name());
    }
    present
}

fn run_all(inputs: &[Input], tools: &[Tool], kind: Kind) -> io::Result<Vec<Done>> {
    let jobs: Vec<Job> = (0..inputs.len())
        .flat_map(|input| WIDTHS.into_iter().flat_map(move |width| tools.iter().map(move |&tool| Job { input, tool, width })))
        .collect();
    parallel(&jobs, |&job| run(job, &inputs[job.input], kind))
}

/// Does every job on all cores; each worker takes the next job until none is left.
fn parallel<J: Sync, D: Send>(jobs: &[J], work: impl Fn(&J) -> io::Result<D> + Sync) -> io::Result<Vec<D>> {
    let next = AtomicUsize::new(0);
    let workers = thread::available_parallelism().map_or(4, usize::from);
    let worker = || -> io::Result<Vec<D>> {
        let mut done = Vec::new();
        while let Some(job) = jobs.get(next.fetch_add(1, Ordering::Relaxed)) {
            done.push(work(job)?);
            if done.len() % 200 == 0 {
                eprintln!("{} of {} runs started", next.load(Ordering::Relaxed).min(jobs.len()), jobs.len());
            }
        }
        Ok(done)
    };
    thread::scope(|scope| {
        let handles: Vec<_> = (0..workers).map(|_| scope.spawn(worker)).collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap_or_else(|_| Err(io::Error::other("a worker panicked"))))
            .collect::<io::Result<Vec<_>>>()
    })
    .map(|done| done.into_iter().flatten().collect())
}

/// vt-mermaid measures its flowcharts, state, class and ER diagrams exactly; a sequence diagram has only the counts read from its
/// text.
fn run(job: Job, input: &Input, kind: Kind) -> io::Result<Done> {
    let run = job.tool.run(&input.source, job.width)?;
    let measured = job.tool == Tool::VtMermaid && kind.measured() && run.status == Status::Rendered;
    let exact = if measured { exact(&input.source, job.width)? } else { None };
    Ok(Done { job, run, exact })
}

fn exact(source: &str, width: u16) -> io::Result<Option<Exact>> {
    let run = tool::measure(source, width)?;
    Ok(if run.status == Status::Rendered { serde_json::from_str(&run.text).ok() } else { None })
}

fn measure_in_process(width: u16) -> Result<(), Box<dyn Error>> {
    let mut source = String::new();
    io::stdin().read_to_string(&mut source)?;
    let measures = vt_mermaid::measure(&source, width)?;
    let exact = Exact {
        crossings: measures.crossings,
        bends: measures.bends,
        edge_cells: measures.edge_cells,
        beside_nodes: measures.beside_nodes,
        violations: measures.violations,
    };
    println!("{}", serde_json::to_string(&exact)?);
    Ok(())
}

fn fresh_metrics(done: &[Done], input: usize, expected: Option<&Expected>, kind: Kind) -> BTreeMap<String, BTreeMap<u16, Metrics>> {
    let mut tools: BTreeMap<String, BTreeMap<u16, Metrics>> = BTreeMap::new();
    for done in done.iter().filter(|done| done.job.input == input) {
        let metrics = Metrics::of(&done.run, done.job.width, expected, done.exact.clone(), kind);
        tools.entry(done.job.tool.name().to_owned()).or_default().insert(done.job.width, metrics);
    }
    tools
}

fn times(done: &[Done]) -> Times {
    let mut times = Times::new();
    for done in done {
        times.entry((done.job.tool, done.job.width)).or_default().push(done.run.millis);
    }
    times
}

fn write_gallery(path: &Path, inputs: &[Input], results: &[InputResult], done: &[Done]) -> io::Result<()> {
    let mut runs: Vec<BTreeMap<(Tool, u16), &Run>> = vec![BTreeMap::new(); inputs.len()];
    for done in done {
        runs[done.job.input].insert((done.job.tool, done.job.width), &done.run);
    }
    let entries: Vec<gallery::Entry<'_>> = inputs
        .iter()
        .zip(results)
        .zip(runs)
        .map(|((input, result), runs)| gallery::Entry { source: &input.source, result, runs })
        .collect();
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    fs::write(path, gallery::page(&entries, &WIDTHS))
}
