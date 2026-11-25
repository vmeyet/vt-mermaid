//! vt-mermaid's flowcharts measured against other text renderers: `cargo run --release --example scoreboard`.
//!
//! Options: `--sample <n>` runs one input in n, `--tools <a,b>` runs only those tools and keeps the others' last results,
//! `--gallery <file.html>` writes the gallery there instead of `target/scoreboard/gallery.html`.
//! [`README.md`](README.md) defines the metrics; `install.sh` installs the competitors.

mod expected;
mod gallery;
mod metrics;
mod results;
mod tool;

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use expected::Expected;
use metrics::{Exact, Metrics};
use results::{Counts, InputResult, Times};
use tool::{Run, Status, Tool};

const WIDTHS: [u16; 2] = [80, 100];

#[derive(Debug)]
struct Options {
    sample: Option<usize>,
    tools: Vec<Tool>,
    gallery: PathBuf,
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
        _ => scoreboard(&Options::parse(&args)?),
    }
}

fn scoreboard(options: &Options) -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let inputs = inputs(&root.join("corpus/flowchart"), options.sample)?;
    let expected = expected::read(&root.join("oracle/results/flowchart.json"))?;
    let tools = installed(&options.tools);
    let done = run_all(&inputs, &tools)?;
    let folder = root.join("scoreboard/results/inputs");
    fs::create_dir_all(&folder)?;
    let results = inputs
        .iter()
        .enumerate()
        .map(|(index, input)| {
            let expected = expected.get(&input.path);
            InputResult::merged(&folder, &input.path, expected.map(Counts::of), fresh_metrics(&done, index, expected))
        })
        .collect::<io::Result<Vec<_>>>()?;
    for result in &results {
        result.write(&folder)?;
    }
    let summary = results::summary(&results, &WIDTHS, &times(&done), options.sample);
    fs::write(root.join("scoreboard/results/summary.md"), &summary)?;
    write_gallery(&options.gallery, &inputs, &results, &done)?;
    println!("{summary}\ngallery: {}", options.gallery.display());
    Ok(())
}

impl Options {
    fn parse(args: &[String]) -> Result<Self, Box<dyn Error>> {
        let mut options = Self { sample: None, tools: Tool::ALL.to_vec(), gallery: default_gallery() };
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
                "--gallery" => options.gallery = PathBuf::from(value),
                _ => return Err(format!("unknown option {flag}").into()),
            }
        }
        Ok(options)
    }
}

fn default_gallery() -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target"), PathBuf::from);
    target.join("scoreboard/gallery.html")
}

/// The flowcharts of the corpus in name order, or one in `sample` of them.
fn inputs(folder: &Path, sample: Option<usize>) -> io::Result<Vec<Input>> {
    let mut paths: Vec<PathBuf> = fs::read_dir(folder)?.map(|entry| entry.map(|entry| entry.path())).collect::<io::Result<_>>()?;
    paths.sort();
    paths
        .into_iter()
        .step_by(sample.unwrap_or(1))
        .map(|path| {
            let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            Ok(Input { source: fs::read_to_string(&path)?, path: format!("flowchart/{name}") })
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

/// Runs every job on all cores; each worker takes the next job until none is left.
fn run_all(inputs: &[Input], tools: &[Tool]) -> io::Result<Vec<Done>> {
    let jobs: Vec<Job> = (0..inputs.len())
        .flat_map(|input| WIDTHS.into_iter().flat_map(move |width| tools.iter().map(move |&tool| Job { input, tool, width })))
        .collect();
    let next = AtomicUsize::new(0);
    let workers = thread::available_parallelism().map_or(4, usize::from);
    let worker = || -> io::Result<Vec<Done>> {
        let mut done = Vec::new();
        while let Some(&job) = jobs.get(next.fetch_add(1, Ordering::Relaxed)) {
            done.push(run(job, &inputs[job.input])?);
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

fn run(job: Job, input: &Input) -> io::Result<Done> {
    let run = job.tool.run(&input.source, job.width)?;
    let exact = if job.tool == Tool::VtMermaid && run.status == Status::Rendered { exact(&input.source, job.width)? } else { None };
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
    let exact =
        Exact { crossings: measures.crossings, bends: measures.bends, edge_cells: measures.edge_cells, violations: measures.violations };
    println!("{}", serde_json::to_string(&exact)?);
    Ok(())
}

fn fresh_metrics(done: &[Done], input: usize, expected: Option<&Expected>) -> BTreeMap<String, BTreeMap<u16, Metrics>> {
    let mut tools: BTreeMap<String, BTreeMap<u16, Metrics>> = BTreeMap::new();
    for done in done.iter().filter(|done| done.job.input == input) {
        let metrics = Metrics::of(&done.run, done.job.width, expected, done.exact.clone());
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
