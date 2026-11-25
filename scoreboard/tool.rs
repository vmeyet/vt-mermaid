//! The renderers on the board and how each one is run: always in a child process, so a hang is killed at the deadline
//! and a crash is told apart from a refusal.

use std::env;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use vt_mermaid::GlyphSet;

const DEADLINE: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(2);
/// Exit code of a child that refused the input; a panicking Rust child exits with 101.
const REFUSED: i32 = 2;
const BEAUTIFUL_MERMAID: &str = include_str!("beautiful-mermaid.mjs");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Tool {
    VtMermaid,
    MermaidText,
    MermaidAscii,
    BeautifulMermaid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Status {
    Rendered,
    Refused,
    Crashed,
    TimedOut,
}

/// What a tool made of one input: its drawing when it rendered, else its first line of error.
#[derive(Debug, Clone)]
pub(crate) struct Run {
    pub(crate) status: Status,
    pub(crate) text: String,
    pub(crate) millis: u64,
}

impl Tool {
    pub(crate) const ALL: [Self; 4] = [Self::VtMermaid, Self::MermaidText, Self::MermaidAscii, Self::BeautifulMermaid];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::VtMermaid => "vt-mermaid",
            Self::MermaidText => "mermaid-text",
            Self::MermaidAscii => "mermaid-ascii",
            Self::BeautifulMermaid => "beautiful-mermaid",
        }
    }

    pub(crate) fn version(self) -> &'static str {
        match self {
            Self::VtMermaid => env!("CARGO_PKG_VERSION"),
            Self::MermaidText => "0.58.0",
            Self::MermaidAscii => "1.6.2",
            Self::BeautifulMermaid => "1.1.3",
        }
    }

    pub(crate) fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tool| tool.name() == name)
    }

    /// Whether the tool can run here: the Rust ones always can, the others once `install.sh` put them in the cache.
    pub(crate) fn is_installed(self) -> bool {
        match self {
            Self::VtMermaid | Self::MermaidText => true,
            Self::MermaidAscii => mermaid_ascii().is_file(),
            Self::BeautifulMermaid => node_folder().join("node_modules/beautiful-mermaid").is_dir() && found_on_path("node"),
        }
    }

    /// Draws `source` at most `width` columns wide, in a child process killed at the deadline.
    pub(crate) fn run(self, source: &str, width: u16) -> io::Result<Run> {
        let source = if self == Self::VtMermaid { source } else { without_front_matter(source) };
        self.finish(self.command(width)?, source)
    }

    fn finish(self, mut command: Command, source: &str) -> io::Result<Run> {
        let started = Instant::now();
        let mut child = command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
        let input = child.stdin.take().map(|mut stdin| {
            let source = source.to_owned();
            thread::spawn(move || stdin.write_all(source.as_bytes()))
        });
        let stdout = child.stdout.take().map(|stdout| thread::spawn(move || read_all(stdout)));
        let stderr = child.stderr.take().map(|stderr| thread::spawn(move || read_all(stderr)));
        let exit = wait_until(&mut child, started + DEADLINE)?;
        let millis = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let _broken_pipe_when_the_child_stops_reading = input.map(thread::JoinHandle::join);
        let (stdout, stderr) = (joined(stdout), joined(stderr));
        let status = self.status(exit, &stdout, &stderr);
        let text =
            if status == Status::Rendered { stdout } else { stderr.lines().find(|line| !line.trim().is_empty()).unwrap_or("").to_owned() };
        Ok(Run { status, text, millis })
    }

    /// A child of this very program for the Rust tools, so they run in-process there.
    fn command(self, width: u16) -> io::Result<Command> {
        let width = width.to_string();
        let mut command = match self {
            Self::VtMermaid | Self::MermaidText => {
                let mut command = Command::new(env::current_exe()?);
                command.args(["--render", self.name(), &width]);
                command
            }
            Self::MermaidAscii => {
                let mut command = Command::new(mermaid_ascii());
                command.args(["-f", "-", "--max-width", &width]);
                command
            }
            Self::BeautifulMermaid => {
                let mut command = Command::new("node");
                command.args(["--input-type=module", "--eval", BEAUTIFUL_MERMAID]).current_dir(node_folder());
                command
            }
        };
        command.env_remove("NODE_OPTIONS");
        Ok(command)
    }

    fn status(self, exit: Option<ExitStatus>, stdout: &str, stderr: &str) -> Status {
        let Some(exit) = exit else { return Status::TimedOut };
        if self.crashed(exit, stderr) {
            Status::Crashed
        } else if exit.success() && !stdout.trim().is_empty() {
            Status::Rendered
        } else {
            Status::Refused
        }
    }

    fn crashed(self, exit: ExitStatus, stderr: &str) -> bool {
        let Some(code) = exit.code() else { return true };
        match self {
            Self::VtMermaid | Self::MermaidText | Self::BeautifulMermaid => code != 0 && code != REFUSED,
            Self::MermaidAscii => stderr.contains("panic:") || stderr.contains("fatal error:"),
        }
    }
}

/// vt-mermaid's exact measures of `source` as JSON, from a child as every run is, so a hang there is killed too.
pub(crate) fn measure(source: &str, width: u16) -> io::Result<Run> {
    let mut command = Command::new(env::current_exe()?);
    command.args(["--measure", &width.to_string()]);
    Tool::VtMermaid.finish(command, source)
}

/// The child side of [`Tool::run`] for the Rust tools: the drawing on stdout, or the reason on stderr and exit 2.
pub(crate) fn render_in_process(name: &str, width: u16) -> Result<(), Box<dyn std::error::Error>> {
    let mut source = String::new();
    io::stdin().read_to_string(&mut source)?;
    let drawn = match Tool::named(name) {
        Some(Tool::VtMermaid) => vt_mermaid::render(&source, width).map(|canvas| text_of(&canvas)).map_err(|error| error.to_string()),
        Some(Tool::MermaidText) => mermaid_text(&source, width),
        _ => return Err(format!("{name} does not run in-process").into()),
    };
    match drawn {
        Ok(text) => println!("{text}"),
        Err(reason) => {
            eprintln!("{reason}");
            std::process::exit(REFUSED);
        }
    }
    Ok(())
}

pub(crate) fn text_of(canvas: &vt_mermaid::Canvas<vt_mermaid::Role>) -> String {
    canvas.lines(GlyphSet::Unicode).collect::<Vec<_>>().join("\n")
}

fn mermaid_text(source: &str, width: u16) -> Result<String, String> {
    let options = mermaid_text::RenderOptions { max_width: Some(usize::from(width)), max_width_strict: true, ..Default::default() };
    mermaid_text::render_with_options(source, &options).map_err(|error| error.to_string())
}

/// mermaid-text, mermaid-ascii and beautiful-mermaid refuse a front matter block (`---` … `---`), which is not what is measured.
fn without_front_matter(source: &str) -> &str {
    source.strip_prefix("---\n").and_then(|rest| rest.split_once("\n---\n")).map_or(source, |(_, body)| body)
}

fn wait_until(child: &mut Child, deadline: Instant) -> io::Result<Option<ExitStatus>> {
    loop {
        if let Some(exit) = child.try_wait()? {
            return Ok(Some(exit));
        }
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            return Ok(None);
        }
        thread::sleep(POLL);
    }
}

fn read_all(mut from: impl Read) -> String {
    let mut bytes = Vec::new();
    let _partial_output_is_still_measured = from.read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}

fn joined(reader: Option<thread::JoinHandle<String>>) -> String {
    reader.and_then(|reader| reader.join().ok()).unwrap_or_default()
}

/// Where `install.sh` puts the tools.
pub(crate) fn cache() -> PathBuf {
    env::var_os("VT_SCOREBOARD_CACHE")
        .map(PathBuf::from)
        .or_else(|| env::var_os("XDG_CACHE_HOME").map(|cache| Path::new(&cache).join("vt-scoreboard")))
        .or_else(|| env::var_os("HOME").map(|home| Path::new(&home).join(".cache/vt-scoreboard")))
        .unwrap_or_default()
}

fn mermaid_ascii() -> PathBuf {
    cache().join("bin/mermaid-ascii")
}

fn node_folder() -> PathBuf {
    cache().join("node")
}

fn found_on_path(program: &str) -> bool {
    env::var_os("PATH").is_some_and(|path| env::split_paths(&path).any(|folder| folder.join(program).is_file()))
}
