//! The comparison page, `docs/comparison.md`: every kind's board, vt-mermaid's speed and agreement with mermaid, drawings
//! side by side and the kinds only other tools draw, written from one run.

use std::time::Duration;

use crate::WIDTHS;
use crate::expected::Kind;
use crate::machine::Machine;
use crate::others::Others;
use crate::results::{self, Board, mean_score};
use crate::speed::Speed;
use crate::tool::{Run, Status, Tool};

/// The opening example of a syntax page in mermaid's docs: what a reader tries first, picked by that rule alone.
pub(crate) const SAMPLES: [(Kind, &str); 3] = [
    (Kind::Sequence, "sequence/docs--syntax-sequenceDiagram--L12.mmd"),
    (Kind::State, "state/docs--syntax-stateDiagram--L15.mmd"),
    (Kind::Er, "er/docs--syntax-entityRelationshipDiagram--L14.mmd"),
];

/// A case vt-mermaid draws poorly, drawn again on every run so the page shows when it is fixed.
#[derive(Debug)]
pub(crate) struct WeakSpot {
    pub(crate) title: &'static str,
    pub(crate) why: &'static str,
    pub(crate) source: &'static str,
}

pub(crate) const WEAK_SPOTS: [WeakSpot; 3] = [
    WeakSpot {
        title: "ER crow's feet are cramped",
        why: "Each end of a relationship takes two rows of marks, and two relationships leaving one entity put their marks side by side.\nA label can sit across its own line.",
        source: "erDiagram\n  CUSTOMER ||--o{ ORDER : places\n  CUSTOMER }|..|{ DELIVERY-ADDRESS : uses",
    },
    WeakSpot {
        title: "Labels on a cycle of two states",
        why: "When two states point at each other, both lines and both labels share the narrow gap between them.\nA label can cover one of the lines.",
        source: "stateDiagram-v2\n  Idle --> Running : start\n  Running --> Idle : stop",
    },
    WeakSpot {
        title: "Messages to oneself are small",
        why: "A message from a participant to itself is a loop two columns wide under its label, easy to miss in a busy diagram.",
        source: "sequenceDiagram\n  Alice->>Alice: think\n  Alice->>Bob: hi\n  Bob-->>Bob: ponder over it",
    },
];

/// One sample input as each tool drew it; a drawing is none when the tool is not installed here.
#[derive(Debug)]
pub(crate) struct Sample {
    pub(crate) input: &'static str,
    pub(crate) source: String,
    pub(crate) ours: Option<Run>,
    pub(crate) rival: Option<(Tool, Option<Run>)>,
}

/// Everything one run measured.
#[derive(Debug)]
pub(crate) struct Page {
    pub(crate) boards: Vec<Board>,
    pub(crate) installed: Vec<Tool>,
    pub(crate) others: Vec<Others>,
    pub(crate) speeds: Vec<Speed>,
    pub(crate) samples: Vec<Sample>,
    pub(crate) weak_spots: Vec<Run>,
    pub(crate) machine: Machine,
    pub(crate) mermaid: String,
    pub(crate) minutes: u64,
}

pub(crate) fn markdown(page: &Page) -> String {
    [
        intro(page),
        at_a_glance(page),
        rules(page),
        agreement(page),
        speed(page),
        side_by_side(page),
        others(page),
        weak_spots(page),
        details(page),
        tools(page),
        reproduce(page),
    ]
    .join("\n")
}

fn intro(page: &Page) -> String {
    let machine = &page.machine;
    format!(
        "# How vt-mermaid compares\n\n\
         vt-mermaid and three other tools that draw Mermaid as text, run on every flowchart, sequence, state, class and ER diagram in mermaid's own test [corpus](../corpus/README.md).\n\
         This page is written by the [scoreboard](../scoreboard/README.md) from one run: do not edit it by hand.\n\
         The run was on {}, took {} minutes, on {} ({} cores) with {}.\n\n\
         - [At a glance](#at-a-glance)\n\
         - [It's our own metric: here are its rules](#its-our-own-metric-here-are-its-rules)\n\
         - [Agreement with mermaid](#agreement-with-mermaid)\n\
         - [Speed](#speed)\n\
         - [Side by side](#side-by-side)\n\
         - [Kinds other tools draw that we don't yet](#kinds-other-tools-draw-that-we-dont-yet)\n\
         - [Known weak spots](#known-weak-spots)\n\
         - [Details per kind](#details-per-kind)\n\
         - [Tools and machine](#tools-and-machine)\n\
         - [Reproduce](#reproduce)\n",
        machine.date, page.minutes, machine.cpu, machine.cores, machine.os
    )
}

fn at_a_glance(page: &Page) -> String {
    let mut text = String::from(
        "## At a glance\n\nMean score out of 100 per diagram kind, higher is better.\n\
         The [rules](#its-our-own-metric-here-are-its-rules) say what it counts; a refusal scores 0.\n",
    );
    for width in WIDTHS {
        text.push_str(&format!("\n### {width} columns\n\n{}", score_table(page, width)));
    }
    let missing: Vec<&str> = Tool::ALL.into_iter().filter(|tool| !page.installed.contains(tool)).map(Tool::name).collect();
    if !missing.is_empty() {
        text.push_str(&format!("\nNot installed for this run: {}.\nIts numbers here are its last committed results, and it is left out of the sections that run tools again.\n",
            missing.join(", ")
        ));
    }
    text
}

fn score_table(page: &Page, width: u16) -> String {
    let mut table = header("Tool", page.boards.iter().map(|board| title(board.kind)));
    for tool in Tool::ALL {
        let scores =
            page.boards.iter().map(|board| mean_score(&board.results, tool, width).map_or_else(dash, |score| format!("{score:.1}")));
        table.push_str(&row(&format!("{} {}", tool.name(), tool.version()), scores));
    }
    table
}

fn rules(page: &Page) -> String {
    let scored: Vec<String> = page
        .boards
        .iter()
        .map(|board| {
            let scored = board
                .results
                .iter()
                .filter(|result| result.metrics(Tool::VtMermaid, WIDTHS[0]).is_some_and(|metrics| metrics.score.is_some()));
            format!("{} of {} {}s", scored.count(), board.results.len(), board.kind.noun())
        })
        .collect();
    format!(
        "## It's our own metric: here are its rules\n\n\
         We wrote this score while building vt-mermaid, so it rewards what we chose to care about.\n\
         Read it as a ranking aid, not a verdict.\n\
         Every tool's text output is read by the same code ([`metrics.rs`](../scoreboard/metrics.rs), [`lines.rs`](../scoreboard/lines.rs)), at 80 and 100 columns.\n\
         What a drawing must show comes from what mermaid.js {} itself makes of the input ([oracle](../oracle/README.md)).\n\n\
         Each input scores from 0 to 100, rounded to a whole number:\n\n\
         | Part | Points | Read from the text |\n\
         |---|---|---|\n\
         | Labels | 50 × the share found | Node and edge labels together. A label is found when every word of it appears somewhere in the drawing, markup removed. Full points when mermaid expects no label. |\n\
         | Fits | 20 or 0 | The widest row is at most the columns asked for. |\n\
         | Arrowheads | 15 × (1 − min(\\|drawn − expected\\|, e) ÷ e), with e = max(expected, 1) | Arrowhead glyphs, plus one per edge merged into a shared head. Class diagrams count triangles and diamonds too. ER diagrams count none, so every ER drawing gets these 15 points. |\n\
         | Crossings | 15 ÷ (1 + crossings) | `┼`-like glyphs, without those where merged edges part or gather. In ER diagrams only a `┼` joined on all four sides, as crow's feet use `┼` as a bar. |\n\n\
         - A refusal, a crash or a timeout scores 0.\n\
         - The mean is over the inputs where mermaid's meaning has at least one node: {}.\n\
         - It does not judge how easy a drawing is to follow, nor its size: the per-kind summaries count rows, area and bends on the inputs every tool drew.\n\
         - Words can match elsewhere in the drawing, and a label cut short with `⋯` counts as missing.\n\
         - Glyph counts are proxies; vt-mermaid's exact geometry, in the same summaries, shows how far off they are.\n\
         - The other tools get the source without its front matter (`---` … `---`), which they refuse.\n\
         - The full rules are in the [scoreboard's metrics](../scoreboard/README.md#metrics).\n",
        page.mermaid,
        scored.join(", ")
    )
}

fn agreement(page: &Page) -> String {
    let mut text = format!(
        "## Agreement with mermaid\n\n\
         vt-mermaid's parsers are mermaid {}'s, rewritten by hand.\n\
         Two levels are checked against mermaid.js on the corpus:\n\n\
         - Level A, the verdict: `render` refuses a source as `Invalid` exactly when mermaid refuses it.\n  \
           Measured by this run through `render`, at 100 columns.\n  \
           An input over a size or count limit (`TooLong`, `TooManyNodes`…) stops being read before its verdict is known, so it is not judged.\n\
         - Level B, the meaning: on every input both accept, the same nodes, edges, labels and groups.\n  \
           Checked by the crate's tests (`src/<kind>/parse/agreement.rs`), which fail on any difference, not by this run.\n\n",
        page.mermaid
    );
    text.push_str(&header(
        "Kind",
        ["Inputs", "mermaid accepts", "Read whole", "Level A: same verdict", "Level B: inputs compared"].map(str::to_owned),
    ));
    for (speed, board) in page.speeds.iter().zip(&page.boards) {
        let meanings = board.results.iter().filter(|result| result.expected.is_some()).count();
        let cells = [
            speed.inputs.to_string(),
            speed.mermaid_accepts.to_string(),
            speed.read_whole.to_string(),
            format!("{} of {}", speed.same_verdict, speed.read_whole),
            meanings.to_string(),
        ];
        text.push_str(&row(&title(speed.kind), cells));
    }
    text
}

fn speed(page: &Page) -> String {
    let mut text = String::from(
        "## Speed\n\n### The library alone\n\n\
         `render` timed in-process at 100 columns, one input at a time, with nothing else running.\n\
         An input under 50 ms is timed 5 times and the fastest run kept.\n\
         The last column is what `render` returned; the [README](../README.md#errors-and-limits) explains each error.\n\
         `empty` is a diagram mermaid accepts but finds nothing to draw in, such as a header alone; the scoreboard counts it as refused.\n\n",
    );
    text.push_str(&header("Kind", ["Inputs", "Median", "p95", "Slowest", "Returned"].map(str::to_owned)));
    for speed in &page.speeds {
        let cells = [speed.inputs.to_string(), duration(speed.median), duration(speed.p95), duration(speed.slowest), outcomes(speed)];
        text.push_str(&row(&title(speed.kind), cells));
    }
    text.push_str(&format!(
        "\n### Every tool, start-up included\n\n\
         Wall time of one child process per input at {} columns, as median / p95 / slowest in milliseconds.\n\
         The runs share all cores, and start-up counts: Node starts again for every input of beautiful-mermaid.\n\
         Only the order of magnitude means something here.\n\n",
        WIDTHS[0]
    ));
    text.push_str(&header("Tool", page.boards.iter().map(|board| title(board.kind))));
    for tool in Tool::ALL {
        let cells =
            page.boards.iter().map(|board| board.times.get(&(tool, WIDTHS[0])).map_or_else(|| "not run".to_owned(), |times| spread(times)));
        text.push_str(&row(tool.name(), cells));
    }
    text
}

fn side_by_side(page: &Page) -> String {
    let mut text = format!(
        "## Side by side\n\n\
         The opening example of three syntax pages in mermaid's docs, at {} columns.\n\
         Each is drawn by vt-mermaid, then by the other tool with the best mean score on that kind.\n\
         They are picked by that rule, not by how they look.\n\
         On GitHub, the first block is mermaid's own drawing.\n",
        WIDTHS[0]
    );
    for sample in &page.samples {
        text.push_str(&format!("\n### `{}`\n\n```mermaid\n{}\n```\n\n", sample.input, sample.source.trim_end()));
        text.push_str(&drawing(Tool::VtMermaid, sample.ours.as_ref()));
        match &sample.rival {
            Some((tool, run)) => text.push_str(&drawing(*tool, run.as_ref())),
            None => text.push_str("No other tool drew any input of this kind.\n"),
        }
    }
    text
}

fn others(page: &Page) -> String {
    let tools = &page.installed;
    let mut text = format!(
        "## Kinds other tools draw that we don't yet\n\n\
         vt-mermaid draws five kinds; for every other kind mermaid knows, `render` returns `Unsupported`.\n\
         Every corpus input of those kinds was run through each tool at {} columns.\n\
         Drawn means the tool exited without error and printed something: the drawing was not checked against mermaid's meaning.\n\n",
        WIDTHS[0]
    );
    let drawn_by_some: Vec<&Others> = page.others.iter().filter(|others| others.drawn.values().any(|&count| count > 0)).collect();
    text.push_str(&header("Kind", std::iter::once("Inputs".to_owned()).chain(tools.iter().map(|tool| tool.name().to_owned()))));
    for others in &drawn_by_some {
        let counts = tools.iter().map(|tool| others.drawn.get(tool).map_or_else(dash, usize::to_string));
        text.push_str(&row(&others.kind, std::iter::once(others.inputs.to_string()).chain(counts)));
    }
    let none: Vec<&str> =
        page.others.iter().filter(|others| others.drawn.values().all(|&count| count == 0)).map(|others| others.kind.as_str()).collect();
    if !none.is_empty() {
        text.push_str(&format!("\nNo tool drew any input of: {}.\n", none.join(", ")));
    }
    let gaps: Vec<String> = Tool::ALL
        .into_iter()
        .filter_map(|tool| {
            let none: Vec<&str> = page.boards.iter().filter(|board| draws_none(board, tool)).map(|board| board.kind.noun()).collect();
            (!none.is_empty()).then(|| format!("{} draws no {}", tool.name(), none.join(" and no ")))
        })
        .collect();
    if !gaps.is_empty() {
        text.push_str(&format!("\nAmong the five kinds on the board, {}.\n", gaps.join("; ")));
    }
    text
}

/// Whether the tool ran on the board's inputs and drew none of them.
fn draws_none(board: &Board, tool: Tool) -> bool {
    let ran: Vec<_> = board.results.iter().filter_map(|result| result.metrics(tool, WIDTHS[0])).collect();
    !ran.is_empty() && ran.iter().all(|metrics| metrics.status != Status::Rendered)
}

fn weak_spots(page: &Page) -> String {
    let mut text = format!("## Known weak spots\n\nDrawn by vt-mermaid at {} columns on this run, so a fix shows here.\n", WIDTHS[0]);
    for (spot, run) in WEAK_SPOTS.iter().zip(&page.weak_spots) {
        text.push_str(&format!("\n### {}\n\n{}\n\n```mermaid\n{}\n```\n\n{}", spot.title, spot.why, spot.source, block(run)));
    }
    text
}

fn details(page: &Page) -> String {
    let mut text = format!(
        "## Details per kind\n\n\
         At {} columns; the [scoreboard's metrics](../scoreboard/README.md#metrics) define every column.\n\
         Median and p95 are wall times as above.\n\
         Each kind's summary adds 100 columns, layout counts on the inputs every tool drew, and vt-mermaid's exact geometry.\n",
        WIDTHS[0]
    );
    for board in &page.boards {
        let meanings = board.results.iter().filter(|result| result.expected.is_some()).count();
        let summary = match board.kind.results() {
            "" => "../scoreboard/results/summary.md".to_owned(),
            folder => format!("../scoreboard/results/{folder}/summary.md"),
        };
        text.push_str(&format!(
            "\n### {}\n\n{} inputs, {meanings} with a meaning from mermaid; [full summary]({summary}).\n\n{}",
            title(board.kind),
            board.results.len(),
            results::tools_table(&board.results, WIDTHS[0], &board.times)
        ));
    }
    text
}

fn tools(page: &Page) -> String {
    let machine = &page.machine;
    let mut text = String::from("## Tools and machine\n\n| Tool | Version | Runs as |\n|---|---|---|\n");
    for tool in Tool::ALL {
        text.push_str(&format!("| [{}]({}) | {} | {} |\n", tool.name(), tool.home(), tool.version(), tool.runs_as()));
    }
    text.push_str(&format!("\n- Date: {}\n- CPU: {}, {} cores\n- OS: {}\n- Rust: {}\n- Node: {}\n- Corpus and oracle: mermaid {}\n- Deadline: 10 seconds per input; a tool still running is killed and counts as timed out.\n",
        machine.date, machine.cpu, machine.cores, machine.os, machine.rustc, machine.node, page.mermaid
    ));
    text
}

fn reproduce(page: &Page) -> String {
    format!(
        "## Reproduce\n\n\
         ```sh\n\
         scoreboard/install.sh\n\
         cargo run --release --example scoreboard -- --markdown docs/comparison.md\n\
         ```\n\n\
         - Needs Rust (the toolchain in `rust-toolchain.toml`), `curl` and `shasum` for mermaid-ascii, and Node.js with npm for beautiful-mermaid.\n\
         - `install.sh` is the only step that uses the network.\n  \
           It installs the pinned versions outside the repository, in `~/.cache/vt-scoreboard`, and checks mermaid-ascii's sha256.\n\
         - A tool that is not installed is skipped and keeps its last committed results.\n\
         - The run also rewrites `scoreboard/results/`, and took {} minutes here.\n\
         - Scores do not change between runs on the same code, except for a few flowcharts that mermaid-text 0.58.0 draws differently each time; times and the machine do.\n",
        page.minutes
    )
}

fn drawing(tool: Tool, run: Option<&Run>) -> String {
    let name = format!("{} {}", tool.name(), tool.version());
    match run {
        None => format!("**{name}** is not installed on this run.\n\n"),
        Some(run) if run.status == Status::Rendered => format!("**{name}**\n\n{}\n", block(run)),
        Some(run) => format!("**{name}** {}: `{}`\n\n", status(run.status), run.text.replace('`', "'")),
    }
}

fn block(run: &Run) -> String {
    if run.status != Status::Rendered {
        return format!("{}: `{}`\n", status(run.status), run.text.replace('`', "'"));
    }
    let rows: Vec<&str> = run.text.lines().map(str::trim_end).collect();
    format!("```text\n{}\n```\n", rows.join("\n").trim_matches('\n'))
}

fn status(status: Status) -> &'static str {
    match status {
        Status::Rendered => "drew it",
        Status::Refused => "refused it",
        Status::Crashed => "crashed",
        Status::TimedOut => "timed out",
    }
}

/// `drawn` first, then each error by how often it came.
fn outcomes(speed: &Speed) -> String {
    let mut outcomes: Vec<(&String, &usize)> = speed.outcomes.iter().collect();
    outcomes.sort_by_key(|&(name, count)| (name != "drawn", std::cmp::Reverse(*count)));
    outcomes.iter().map(|(name, count)| format!("{name} {count}")).collect::<Vec<_>>().join(", ")
}

fn spread(times: &[u64]) -> String {
    let mut sorted = times.to_vec();
    sorted.sort_unstable();
    let at = |percent: usize| sorted.get(sorted.len().saturating_sub(1) * percent / 100).map_or_else(dash, u64::to_string);
    format!("{} / {} / {}", at(50), at(95), at(100))
}

fn duration(duration: Duration) -> String {
    let micros = duration.as_micros();
    if micros < 1000 { format!("{micros} µs") } else { format!("{:.1} ms", duration.as_secs_f64() * 1000.0) }
}

fn title(kind: Kind) -> String {
    match kind {
        Kind::Flowchart => "Flowchart",
        Kind::Sequence => "Sequence",
        Kind::State => "State",
        Kind::Class => "Class",
        Kind::Er => "ER",
    }
    .to_owned()
}

fn header(first: &str, columns: impl IntoIterator<Item = String>) -> String {
    let columns: Vec<String> = columns.into_iter().collect();
    format!("| {first} | {} |\n|---|{}\n", columns.join(" | "), "--:|".repeat(columns.len()))
}

fn row(first: &str, cells: impl IntoIterator<Item = String>) -> String {
    format!("| {first} | {} |\n", cells.into_iter().collect::<Vec<_>>().join(" | "))
}

fn dash() -> String {
    "–".to_owned()
}
