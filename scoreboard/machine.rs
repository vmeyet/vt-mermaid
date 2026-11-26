//! The day, machine and toolchain of a run, for the comparison page, read from the system's own commands.

use std::fs;
use std::process::Command;
use std::thread;

#[derive(Debug)]
pub(crate) struct Machine {
    pub(crate) date: String,
    pub(crate) cpu: String,
    pub(crate) cores: usize,
    pub(crate) os: String,
    pub(crate) rustc: String,
    pub(crate) node: String,
}

pub(crate) fn describe() -> Machine {
    Machine {
        date: output("date", &["-u", "+%Y-%m-%d"]),
        cpu: cpu(),
        cores: thread::available_parallelism().map_or(0, usize::from),
        os: os(),
        rustc: output("rustc", &["--version"]),
        node: output("node", &["--version"]),
    }
}

fn cpu() -> String {
    if cfg!(target_os = "macos") {
        return output("sysctl", &["-n", "machdep.cpu.brand_string"]);
    }
    let info = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let model = info.lines().find(|line| line.starts_with("model name")).and_then(|line| line.split_once(':'));
    model.map_or_else(|| "unknown".to_owned(), |(_, model)| model.trim().to_owned())
}

fn os() -> String {
    if cfg!(target_os = "macos") { format!("macOS {}", output("sw_vers", &["-productVersion"])) } else { output("uname", &["-sr"]) }
}

/// What the command printed, trimmed, or `unknown` when it is missing or fails.
fn output(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}
