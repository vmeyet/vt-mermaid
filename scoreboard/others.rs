//! The kinds vt-mermaid does not draw yet, each input run through every tool, to show which tool draws what.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use crate::expected::Kind;
use crate::tool::{Status, Tool};

/// How many inputs of one corpus kind each tool drew.
#[derive(Debug)]
pub(crate) struct Others {
    pub(crate) kind: String,
    pub(crate) inputs: usize,
    pub(crate) drawn: BTreeMap<Tool, usize>,
}

#[derive(Debug)]
struct Input {
    kind: String,
    source: String,
}

/// Every corpus kind but the five on the board, with what each of `tools` drew of it at the narrower width.
pub(crate) fn drawn(corpus: &Path, tools: &[Tool]) -> io::Result<Vec<Others>> {
    let inputs = inputs(corpus)?;
    let jobs: Vec<(usize, Tool)> = (0..inputs.len()).flat_map(|input| tools.iter().map(move |&tool| (input, tool))).collect();
    let done = crate::parallel(&jobs, |&(input, tool)| Ok((input, tool, tool.run(&inputs[input].source, crate::WIDTHS[0])?.status)))?;
    let mut kinds: BTreeMap<&str, Others> = BTreeMap::new();
    for input in &inputs {
        let others = kinds.entry(&input.kind).or_insert_with(|| Others { kind: input.kind.clone(), inputs: 0, drawn: BTreeMap::new() });
        others.inputs += 1;
    }
    for (input, tool, status) in done {
        let drawn = kinds.get_mut(inputs[input].kind.as_str()).map(|others| others.drawn.entry(tool).or_default());
        if let Some(count) = drawn {
            *count += usize::from(status == Status::Rendered);
        }
    }
    Ok(kinds.into_values().collect())
}

fn inputs(corpus: &Path) -> io::Result<Vec<Input>> {
    let mut paths = Vec::new();
    for folder in fs::read_dir(corpus)? {
        let folder = folder?.path();
        let kind = folder.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        if !folder.is_dir() || Kind::ALL.iter().any(|on_board| on_board.name() == kind) {
            continue;
        }
        for file in fs::read_dir(&folder)? {
            paths.push((kind.clone(), file?.path()));
        }
    }
    paths.sort();
    paths.into_iter().map(|(kind, path)| Ok(Input { kind, source: fs::read_to_string(path)? })).collect()
}
