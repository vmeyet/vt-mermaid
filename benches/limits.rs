//! How long `render` takes on a typical flowchart and sequence diagram and on inputs built at each limit:
//! `cargo bench --bench limits`.

use divan::black_box;
use vt_mermaid::{Canvas, Error, MAX_EDGES, MAX_MESSAGES, MAX_NODES, MAX_PARTICIPANTS, MAX_SOURCE_BYTES};

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

/// 64 KiB of edges: the parser lexes it all, then refuses it at the node limit.
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

/// The edge limit on 20 nodes, every node joined to five others: the grid is small and every edge crosses others.
#[divan::bench(sample_count = 3, sample_size = 1)]
fn edges_at_the_limit() -> Result<Canvas, Error> {
    render(&dense(20))
}

/// A sideways tree of 133 nodes whose 121 leaves are 13 lines tall: about 233000 cells to route on.
#[divan::bench(sample_count = 3, sample_size = 1)]
fn area_at_the_limit() -> Result<Canvas, Error> {
    render(&tall_tree())
}

/// The slowest input fuzzing found, a mutation of `corpus/flowchart/e2e--platform-dev-diagrams-performance-flowcharts-medium3.mmd`
/// mended where it broke mermaid's grammar: 19 subgraphs and 149 edges.
#[divan::bench(sample_count = 1, sample_size = 1)]
fn found_by_fuzzing() -> Result<Canvas, Error> {
    render(include_str!("inputs/fuzzed-subgraphs.mmd"))
}

/// The edge limit as a chain of 100 nodes and 51 edges from its first node to its last: the most layers a link
/// passes, about 5000 places kept for links, on a grid 600 rows tall.
#[divan::bench(sample_count = 3, sample_size = 1)]
fn long_links_at_the_limit() -> Result<Canvas, Error> {
    let chain: String = (1..100).map(|index| format!("  n{} --> n{index}\n", index - 1)).collect();
    let long: String = "  n0 --> n99\n".repeat(MAX_EDGES - 99);
    render(&format!("flowchart TD\n{chain}{long}"))
}

#[divan::bench]
fn checkout() -> Result<Canvas, Error> {
    render(include_str!("../tests/sequence/checkout.mmd"))
}

/// Both sequence limits: every participant, and every message going from the first to the last, labelled.
#[divan::bench(sample_count = 3, sample_size = 1)]
fn sequence_at_the_limits() -> Result<Canvas, Error> {
    render(&across(MAX_MESSAGES))
}

/// The same at 80 columns: no placement fits, so every one of them is tried before the drawing is refused.
#[divan::bench(sample_count = 3, sample_size = 1)]
fn sequence_too_wide_at_the_limits() -> Result<Canvas, Error> {
    vt_mermaid::render(black_box(&across(MAX_MESSAGES)), 80)
}

/// Blocks nested as deep as the message limit allows, around one message.
#[divan::bench(sample_count = 3, sample_size = 1)]
fn sequence_nested_at_the_limit() -> Result<Canvas, Error> {
    let depth = (MAX_MESSAGES - 1) / 2;
    render(&format!("sequenceDiagram\n{}  A->>B: deep\n{}", "  loop again\n".repeat(depth), "  end\n".repeat(depth)))
}

/// 64 KiB of messages: the parser reads it all, then the db refuses it at the message limit.
#[divan::bench(sample_count = 10)]
fn sequence_source_at_the_limit() -> Result<Canvas, Error> {
    let source = format!("sequenceDiagram\n{}", "  Alice->>Bob: hello there\n".repeat(3_000));
    render(&source.chars().take(MAX_SOURCE_BYTES).collect::<String>())
}

fn across(messages: usize) -> String {
    let participants: String = (0..MAX_PARTICIPANTS).map(|index| format!("  participant p{index} as Participant {index}\n")).collect();
    let last = MAX_PARTICIPANTS - 1;
    let body: String = (0..messages).map(|index| format!("  p0->>p{last}: message {index} crossing every lifeline\n")).collect();
    format!("sequenceDiagram\n{participants}{body}")
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
