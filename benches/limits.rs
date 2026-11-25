//! How long `render` takes on a typical flowchart and on inputs built at each limit: `cargo bench --bench limits`.

use divan::black_box;
use vt_mermaid::{Canvas, Error, MAX_EDGES, MAX_NODES, MAX_SOURCE_BYTES};

/// The widest canvas vt-canvas draws, so no input here is refused for its width.
const WIDTH: u16 = 1_000;

/// `cargo test --all-targets` runs this without `--bench`, and the inputs at the limits take seconds each.
fn main() {
    if std::env::args().any(|arg| arg == "--bench") {
        divan::main();
    }
}

fn render(source: &str) -> Result<Canvas, Error> {
    vt_mermaid::render(black_box(source), WIDTH)
}

#[divan::bench]
fn cached() -> Result<Canvas, Error> {
    render(include_str!("../tests/spike/cached.mmd"))
}

/// 64 KiB of edges: the parser reads it all before the node limit refuses it.
#[divan::bench(sample_count = 10)]
fn source_at_the_limit() -> Result<Canvas, Error> {
    render(&at_the_length_limit(&(0..).map(|index| format!("  n{index} --> n{}\n", index + 1)).take(5_000).collect::<String>()))
}

/// The node limit, each node with an edge but for the last 50: one more edge would be over the edge limit.
#[divan::bench(sample_count = 3, sample_size = 1)]
fn nodes_at_the_limit() -> Result<Canvas, Error> {
    let nodes: String = (0..MAX_NODES).map(|index| format!("  n{index}[Node {index}]\n")).collect();
    let edges: String = (1..=MAX_EDGES).map(|index| format!("  n{} --> n{index}\n", (index - 1) / 2)).collect();
    render(&format!("flowchart TD\n{nodes}{edges}"))
}

/// The edge limit on 20 nodes, every node joined to five others: mmdr's edge cleanup grows with edges, not nodes.
#[divan::bench(sample_count = 3, sample_size = 1)]
fn edges_at_the_limit() -> Result<Canvas, Error> {
    render(&dense(20))
}

/// A sideways tree of 133 nodes whose 121 leaves are 13 lines tall: about 233000 cells to route on.
#[divan::bench(sample_count = 3, sample_size = 1)]
fn area_at_the_limit() -> Result<Canvas, Error> {
    render(&tall_tree())
}

/// The slowest input fuzzing found, a mutation of `corpus/flowchart/e2e--platform-dev-diagrams-performance-flowcharts-medium3.mmd`:
/// 19 subgraphs and 107 edges, refused by the area limit once mmdr has laid it out.
#[divan::bench(sample_count = 1, sample_size = 1)]
fn found_by_fuzzing() -> Result<Canvas, Error> {
    render(include_str!("inputs/fuzzed-subgraphs.mmd"))
}

fn at_the_length_limit(body: &str) -> String {
    let source = format!("flowchart TD\n{body}");
    source.chars().take(MAX_SOURCE_BYTES).collect()
}

fn dense(nodes: usize) -> String {
    let edges: String = (0..MAX_EDGES)
        .map(|index| {
            let from = index % nodes;
            format!("  n{from} --> n{}\n", (from + 1 + index / nodes * 7) % nodes)
        })
        .collect();
    format!("flowchart TD\n{edges}")
}

fn tall_tree() -> String {
    let label = vec!["n".repeat(30); 13].join("<br>");
    let leaves = (12..133).map(|leaf| (1 + (leaf - 12) / 11, leaf));
    let edges: String =
        (1..12).map(|child| (0, child)).chain(leaves).map(|(parent, child)| format!("  n{parent} --> n{child}\n")).collect();
    let nodes: String = (0..133).map(|index| format!("  n{index}[{label}]\n")).collect();
    format!("flowchart LR\n{nodes}{edges}")
}
