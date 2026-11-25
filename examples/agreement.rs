//! Level-A agreement between mermaid-rs-renderer's parser and the mermaid oracle, per diagram kind.
//!
//! `cargo run --release --example agreement` prints the table.
//! `cargo run --release --example agreement -- flowchart` also lists the cases of that kind where they disagree.
//! "mmdr wrong kind" counts inputs mmdr accepts as another kind, usually a flowchart for a kind it does not know;
//! agreement counts both rejecting, or both accepting as the same kind.

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::panic;
use std::path::Path;

use mermaid_rs_renderer::DiagramKind;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mmdr {
    Accepts { kind: &'static str },
    Rejects,
    Panics,
}

#[derive(Debug)]
struct Case {
    path: String,
    mermaid_accepts: bool,
    mmdr: Mmdr,
}

#[derive(Debug, Default, Clone, Copy)]
struct Tally {
    both_accept: usize,
    both_reject: usize,
    mmdr_only_reject: usize,
    mmdr_only_accept: usize,
    mmdr_wrong_kind: usize,
    mmdr_panics: usize,
    agreed: usize,
}

fn main() -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let listed_kind = std::env::args().nth(1);
    let oracle = oracle_verdicts(&root.join("oracle/results/parse.json"))?;
    panic::set_hook(Box::new(|_| {}));
    let cases = oracle
        .into_iter()
        .map(|(path, mermaid_accepts)| compare(&root.join("corpus"), path, mermaid_accepts))
        .collect::<Result<Vec<_>, _>>()?;
    print_table(&tally_by_kind(&cases));
    if let Some(kind) = listed_kind {
        print_disagreements(&cases, &kind);
    }
    Ok(())
}

fn oracle_verdicts(results: &Path) -> Result<Vec<(String, bool)>, Box<dyn Error>> {
    let parsed: Value = serde_json::from_str(&fs::read_to_string(results)?)?;
    let cases = parsed["cases"].as_object().ok_or("the oracle results have no `cases` object")?;
    cases
        .iter()
        .map(|(path, result)| {
            let accepted = result["accepted"].as_bool().ok_or_else(|| format!("{path}: `accepted` is not a boolean"))?;
            Ok((path.clone(), accepted))
        })
        .collect()
}

fn compare(corpus: &Path, path: String, mermaid_accepts: bool) -> Result<Case, Box<dyn Error>> {
    let source = fs::read_to_string(corpus.join(&path))?;
    Ok(Case { mmdr: mmdr_parse(&source), path, mermaid_accepts })
}

fn mmdr_parse(source: &str) -> Mmdr {
    match panic::catch_unwind(|| mermaid_rs_renderer::parse_mermaid_strict(source).map(|parsed| parsed.graph.kind)) {
        Ok(Ok(kind)) => Mmdr::Accepts { kind: corpus_kind(kind) },
        Ok(Err(_)) => Mmdr::Rejects,
        Err(_) => Mmdr::Panics,
    }
}

fn corpus_kind(kind: DiagramKind) -> &'static str {
    match kind {
        DiagramKind::Flowchart => "flowchart",
        DiagramKind::Class => "class",
        DiagramKind::State => "state",
        DiagramKind::Sequence => "sequence",
        DiagramKind::Er => "er",
        DiagramKind::Pie => "pie",
        DiagramKind::Mindmap => "mindmap",
        DiagramKind::Journey => "journey",
        DiagramKind::Timeline => "timeline",
        DiagramKind::Gantt => "gantt",
        DiagramKind::Requirement => "requirement",
        DiagramKind::GitGraph => "git",
        DiagramKind::C4 => "c4",
        DiagramKind::Sankey => "sankey",
        DiagramKind::Quadrant => "quadrant",
        DiagramKind::ZenUML => "zenuml",
        DiagramKind::Block => "block",
        DiagramKind::Packet => "packet",
        DiagramKind::Kanban => "kanban",
        DiagramKind::Architecture => "architecture",
        DiagramKind::Radar => "radar",
        DiagramKind::Treemap => "treemap",
        DiagramKind::XYChart => "xychart",
    }
}

fn kind_of(path: &str) -> &str {
    path.split('/').next().unwrap_or(path)
}

impl Case {
    fn mmdr_accepts(&self) -> bool {
        matches!(self.mmdr, Mmdr::Accepts { .. })
    }

    fn has_wrong_kind(&self) -> bool {
        matches!(self.mmdr, Mmdr::Accepts { kind } if kind != kind_of(&self.path))
    }

    fn agrees(&self) -> bool {
        self.mermaid_accepts == self.mmdr_accepts() && !self.has_wrong_kind()
    }
}

fn tally_by_kind(cases: &[Case]) -> BTreeMap<&str, Tally> {
    let mut tallies = BTreeMap::new();
    for case in cases {
        for kind in [kind_of(&case.path), "total"] {
            let tally: &mut Tally = tallies.entry(kind).or_default();
            *tally = tally.add(case);
        }
    }
    tallies
}

impl Tally {
    fn add(self, case: &Case) -> Self {
        let (mermaid, mmdr) = (case.mermaid_accepts, case.mmdr_accepts());
        Self {
            both_accept: self.both_accept + usize::from(mermaid && mmdr),
            both_reject: self.both_reject + usize::from(!mermaid && !mmdr),
            mmdr_only_reject: self.mmdr_only_reject + usize::from(mermaid && !mmdr),
            mmdr_only_accept: self.mmdr_only_accept + usize::from(!mermaid && mmdr),
            mmdr_wrong_kind: self.mmdr_wrong_kind + usize::from(case.has_wrong_kind()),
            mmdr_panics: self.mmdr_panics + usize::from(case.mmdr == Mmdr::Panics),
            agreed: self.agreed + usize::from(case.agrees()),
        }
    }

    fn cases(self) -> usize {
        self.both_accept + self.both_reject + self.mmdr_only_reject + self.mmdr_only_accept
    }

    fn agreement_percent(self) -> usize {
        self.agreed * 100 / self.cases().max(1)
    }
}

fn print_table(tallies: &BTreeMap<&str, Tally>) {
    println!(
        "| Kind | Cases | Both accept | Both reject | mmdr-only reject | mmdr-only accept | mmdr wrong kind | mmdr panics | Agreement |"
    );
    println!("|---|--:|--:|--:|--:|--:|--:|--:|--:|");
    let by_kind = tallies.iter().filter(|(kind, _)| **kind != "total");
    for (kind, tally) in by_kind.chain(tallies.get_key_value("total")) {
        println!(
            "| {kind} | {} | {} | {} | {} | {} | {} | {} | {}% |",
            tally.cases(),
            tally.both_accept,
            tally.both_reject,
            tally.mmdr_only_reject,
            tally.mmdr_only_accept,
            tally.mmdr_wrong_kind,
            tally.mmdr_panics,
            tally.agreement_percent()
        );
    }
}

fn print_disagreements(cases: &[Case], kind: &str) {
    println!();
    for case in cases.iter().filter(|case| kind_of(&case.path) == kind && !case.agrees()) {
        println!("mermaid accepts: {}\tmmdr: {:?}\tcorpus/{}", case.mermaid_accepts, case.mmdr, case.path);
    }
}
