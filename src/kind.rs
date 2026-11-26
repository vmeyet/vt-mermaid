//! Which diagram a source holds, as mermaid detects it from the word it opens with, and how that kind is drawn.

use vt_canvas::Canvas;

use crate::error::Error;
use crate::graph::diagram::Diagram;
use crate::settings::Settings;
use crate::source::{self, Prepared};
use crate::{Role, class, er, flowchart, graph, sequence, state};

/// How a kind drawn here is drawn.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Kind {
    /// Read into a graph, then laid out, routed and drawn as every graph is.
    Graph(fn(&Prepared) -> Result<Diagram, Error>),
    /// Drawn its own way.
    Own(fn(&Prepared, Settings) -> Result<Canvas<Role>, Error>),
}

/// A word a source may open with, as a prefix of it, and the kind it names; none for a kind mermaid knows but not
/// drawn here.
#[derive(Debug, Clone, Copy)]
struct Header {
    word: &'static str,
    kind: Option<Kind>,
}

const fn drawn(word: &'static str, kind: Kind) -> Header {
    Header { word, kind: Some(kind) }
}

const fn not_drawn(word: &'static str) -> Header {
    Header { word, kind: None }
}

/// Every header mermaid 12.3.0 knows, in the order its detectors try them.
const HEADERS: [Header; 39] = [
    drawn("graph", Kind::Graph(flowchart::read)),
    drawn("flowchart", Kind::Graph(flowchart::read)),
    drawn("sequenceDiagram", Kind::Own(sequence::render_with)),
    drawn("stateDiagram", Kind::Graph(state::read)),
    drawn("classDiagram", Kind::Graph(class::read)),
    drawn("erDiagram", Kind::Graph(er::read)),
    not_drawn("journey"),
    not_drawn("gantt"),
    not_drawn("pie"),
    not_drawn("quadrantChart"),
    not_drawn("requirementDiagram"),
    not_drawn("gitGraph"),
    not_drawn("C4Context"),
    not_drawn("C4Container"),
    not_drawn("C4Component"),
    not_drawn("C4Dynamic"),
    not_drawn("C4Deployment"),
    not_drawn("mindmap"),
    not_drawn("timeline"),
    not_drawn("zenuml"),
    not_drawn("sankey"),
    not_drawn("xychart"),
    not_drawn("block"),
    not_drawn("packet"),
    not_drawn("kanban"),
    not_drawn("architecture"),
    not_drawn("radar"),
    not_drawn("treemap"),
    not_drawn("info"),
    not_drawn("swimlane"),
    not_drawn("venn"),
    not_drawn("ishikawa"),
    not_drawn("cynefin"),
    not_drawn("wardley"),
    not_drawn("agentflow"),
    not_drawn("eventmodeling"),
    not_drawn("treeView"),
    not_drawn("usecase"),
    not_drawn("railroad"),
];

impl Kind {
    /// The kind of diagram `prepared` holds; refused when it holds none drawn here.
    pub(crate) fn of(prepared: &Prepared) -> Result<Self, Error> {
        let header = HEADERS.iter().find(|header| source::opens_with(&prepared.text, header.word));
        match header {
            Some(Header { kind: Some(kind), .. }) => Ok(*kind),
            Some(Header { kind: None, .. }) => Err(Error::Unsupported { kind: source::header(&prepared.text) }),
            None => Err(Error::Invalid { reason: "no diagram type matches the text".to_owned() }),
        }
    }

    /// Draws `prepared`, a diagram of this kind, as `settings` say.
    pub(crate) fn render(self, prepared: &Prepared, settings: Settings) -> Result<Canvas<Role>, Error> {
        match self {
            Self::Graph(read) => graph::render(&read(prepared)?, settings).map(|drawing| drawing.canvas),
            Self::Own(render) => render(prepared, settings),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use serde_json::Value;

    use super::*;

    fn word(source: &str) -> Option<&'static str> {
        HEADERS.iter().find(|header| source::opens_with(&source::prepare(source).text, header.word)).map(|header| header.word)
    }

    #[test]
    fn the_kind_is_read_after_the_front_matter_and_comments() {
        assert_eq!(word("---\ntitle: a\n---\n%% note\nsequenceDiagram\n  A->>B: hi"), Some("sequenceDiagram"));
        assert_eq!(word("  graph TD\n  A --> B"), Some("graph"));
        assert_eq!(word("%%%%{init: {}}%%\nsequenceDiagram\n  A->>B: hi"), Some("sequenceDiagram"));
    }

    #[test]
    fn a_diagram_of_another_kind_is_unsupported_and_text_without_a_header_invalid() {
        let of = |source: &str| Kind::of(&source::prepare(source)).map(|_| ());

        assert_eq!(of("journey\n  title A day"), Err(Error::Unsupported { kind: "journey".to_owned() }));
        assert!(matches!(of("not a diagram"), Err(Error::Invalid { .. })));
    }

    /// Every kind mermaid accepts in the corpus but those drawn here is told apart from text that is no diagram; only a
    /// radar chart whose front matter closes less indented than it opens, which mermaid's radar parser reads past and
    /// the shared preparation does not, is left out.
    #[test]
    fn every_corpus_diagram_mermaid_accepts_of_a_kind_not_drawn_is_unsupported() {
        let drawn = ["flowchart-v2", "flowchart-elk", "sequence", "stateDiagram", "classDiagram", "er"];
        let unfenced = "radar/e2e--platform-dev-diagrams-knsv-knsv2-03-radar.mmd";
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let oracle: Value = serde_json::from_str(&fs::read_to_string(root.join("oracle/results/parse.json")).unwrap()).unwrap();

        let mut found = Vec::new();
        for (path, verdict) in oracle["cases"].as_object().unwrap() {
            let kind = verdict["kind"].as_str().unwrap_or_default();
            if verdict["accepted"] != true || drawn.contains(&kind) || path == unfenced {
                continue;
            }
            let prepared = source::prepare(&fs::read_to_string(root.join("corpus").join(path)).unwrap());
            if !matches!(Kind::of(&prepared), Err(Error::Unsupported { .. })) {
                found.push(path.clone());
            }
        }

        assert_eq!(found, [""; 0]);
    }
}
