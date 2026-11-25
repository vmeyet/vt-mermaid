//! Flowcharts: mmdr's layout snapped to the grid, edges routed on it, then drawn.

mod diagram;
mod draw;
mod edge_label;
mod label;
mod route;
mod shape;
mod snap;

use vt_canvas::{Canvas, Rect};

use crate::error::{Error, MAX_AREA, MAX_SOURCE_BYTES};
use diagram::Diagram;
use route::Route;

/// A drawn flowchart and where each part went, in canvas cells.
#[derive(Debug, Clone)]
#[cfg_attr(not(test), expect(dead_code, reason = "the parts are read by the level C checks"))]
pub(crate) struct Drawing {
    pub(crate) canvas: Canvas,
    nodes: Vec<Rect>,
    groups: Vec<Rect>,
    routes: Vec<Route>,
    labels: Vec<Option<Rect>>,
}

/// Draws `source` as laid out, or turned top-down when it flows sideways and that is too wide.
pub(crate) fn render(source: &str, max_width: u16) -> Result<Drawing, Error> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(Error::TooLong { bytes: source.len() });
    }
    let diagram = Diagram::read(source)?;
    match drawn(&diagram, max_width) {
        Err(Error::TooWide { .. }) if diagram.is_horizontal() => drawn(&Diagram::read_top_down(source)?, max_width),
        drawing => drawing,
    }
}

fn drawn(diagram: &Diagram, max_width: u16) -> Result<Drawing, Error> {
    let placement = snap::place(diagram);
    check_area(usize::from(placement.size.width), usize::from(placement.size.height))?;
    let (routes, grid) = route::route(diagram, &placement)?;
    let labels = edge_label::place(diagram, &routes, &grid);
    draw::draw(diagram, draw::Parts { nodes: placement.nodes, groups: placement.groups, routes, labels }, max_width)
}

fn check_area(width: usize, height: usize) -> Result<(), Error> {
    if width * height > MAX_AREA { Err(Error::TooLarge { width, height }) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use vt_canvas::{GlyphSet, Point};

    use super::*;

    /// Wide enough for every spike flowchart: the comparison with mermaid-text judges the width.
    const WIDTH: u16 = 400;

    fn spike() -> Vec<(String, String)> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let list = fs::read_to_string(root.join("tests/spike/flowcharts.txt")).unwrap();
        list.lines()
            .map(|path| {
                let name = Path::new(path).file_stem().unwrap().to_string_lossy().into_owned();
                (name, fs::read_to_string(root.join(path)).unwrap())
            })
            .collect()
    }

    fn rows(canvas: &Canvas) -> Vec<String> {
        canvas.rows(GlyphSet::Unicode).map(|runs| runs.into_iter().map(|run| run.text).collect()).collect()
    }

    /// The text of `rect`'s first row as drawn.
    fn text_at(rows: &[String], rect: Rect) -> String {
        rows[usize::from(rect.y)].chars().skip(usize::from(rect.x)).take(usize::from(rect.width)).collect()
    }

    fn contains(rect: Rect, cell: Point) -> bool {
        (rect.x..rect.x + rect.width).contains(&cell.x) && (rect.y..rect.y + rect.height).contains(&cell.y)
    }

    fn inside(inner: Rect, outer: Rect) -> bool {
        outer.x < inner.x
            && outer.y < inner.y
            && inner.x + inner.width < outer.x + outer.width
            && inner.y + inner.height < outer.y + outer.height
    }

    fn disjoint(first: Rect, second: Rect) -> bool {
        first.x + first.width <= second.x
            || second.x + second.width <= first.x
            || first.y + first.height <= second.y
            || second.y + second.height <= first.y
    }

    /// Level C: every node and label drawn whole, every edge joining both its ends, nothing on top of anything else.
    fn violations(source: &str) -> Vec<String> {
        let diagram = Diagram::read(source).unwrap();
        let drawing = render(source, WIDTH).unwrap();
        let rows = rows(&drawing.canvas);
        let rect = |end| match end {
            diagram::End::Node(index) => drawing.nodes[index],
            diagram::End::Group(index) => drawing.groups[index],
        };
        let mut found = Vec::new();
        for (index, (node, &node_rect)) in diagram.nodes.iter().zip(&drawing.nodes).enumerate() {
            let drawn: String = (node_rect.y..node_rect.y + node_rect.height)
                .map(|y| text_at(&rows, Rect::new(node_rect.x, y, node_rect.width, 1)))
                .collect();
            let missing = node.label.lines.iter().filter(|line| !drawn.contains(line.as_str())).count();
            if missing > 0 {
                found.push(format!("node {} has {missing} label lines not drawn whole", node.id));
            }
            if let Some(group) = node.group.filter(|&group| !inside(node_rect, drawing.groups[group])) {
                found.push(format!("node {} is outside its group {}", node.id, diagram.groups[group].id));
            }
            for (other, &other_rect) in drawing.nodes.iter().enumerate().skip(index + 1) {
                if !disjoint(node_rect, other_rect) {
                    found.push(format!("node {} overlaps node {}", node.id, diagram.nodes[other].id));
                }
            }
        }
        for (group, &group_rect) in diagram.groups.iter().zip(&drawing.groups) {
            if let Some(parent) = group.parent.filter(|&parent| !inside(group_rect, drawing.groups[parent])) {
                found.push(format!("group {} is outside its parent {}", group.id, diagram.groups[parent].id));
            }
        }
        for (index, (edge, route)) in diagram.edges.iter().zip(&drawing.routes).enumerate() {
            let name = format!("edge {index} {} → {}", diagram.name(edge.from), diagram.name(edge.to));
            let (from, to) = (rect(edge.from), rect(edge.to));
            let last = *route.cells.last().unwrap();
            let interior = Rect::new(from.x + 1, from.y + 1, from.width - 2, from.height - 2);
            if !contains(from, route.cells[0]) || contains(interior, route.cells[0]) {
                found.push(format!("{name} does not leave from its source's border"));
            }
            if !contains(to, route.end) || last.x.abs_diff(route.end.x) + last.y.abs_diff(route.end.y) != 1 {
                found.push(format!("{name} does not reach its target's border"));
            }
            for cell in &route.cells[1..] {
                if let Some(node) = drawing.nodes.iter().position(|&node_rect| contains(node_rect, *cell)) {
                    found.push(format!("{name} runs through node {}", diagram.nodes[node].id));
                }
            }
            match (&edge.label, drawing.labels[index]) {
                (Some(label), Some(label_rect)) if text_at(&rows, label_rect) != label.lines[0] => {
                    found.push(format!("{name} label is not drawn whole"));
                }
                (Some(_), None) => found.push(format!("{name} label found no room")),
                _ => {}
            }
        }
        let labels: Vec<Rect> = drawing.labels.iter().flatten().copied().collect();
        for (index, &label) in labels.iter().enumerate() {
            let on_node = drawing.nodes.iter().chain(&drawing.groups).any(|&rect| !disjoint(label, rect) && !inside(label, rect));
            if on_node || labels[index + 1..].iter().any(|&other| !disjoint(label, other)) {
                found.push(format!("the label at ({}, {}) overlaps a node, a frame or a label", label.x, label.y));
            }
        }
        found
    }

    #[test]
    fn spike_flowcharts_match_their_snapshots() {
        for (name, source) in spike() {
            let drawing = render(&source, WIDTH).unwrap();
            insta::assert_snapshot!(name, rows(&drawing.canvas).join("\n"));
        }
    }

    /// Level C is not reachable everywhere: a long label between close parallel edges has nowhere to go, and
    /// mmdr misreads L1138's markdown strings into an edge squeezed between two others.
    const KNOWN_VIOLATIONS: [&str; 2] = [
        "docs--community-e2e-tests--L81: edge 2 start → push_develop label found no room",
        "docs--syntax-flowchart--L1138: edge 1 in → d label found no room",
    ];

    #[test]
    fn spike_flowcharts_keep_the_level_c_invariants_but_the_known_ones() {
        let found: Vec<String> = spike()
            .into_iter()
            .flat_map(|(name, source)| violations(&source).into_iter().map(move |found| format!("{name}: {found}")))
            .collect();

        assert_eq!(found, KNOWN_VIOLATIONS);
    }

    #[test]
    fn a_source_at_the_length_limit_is_read() {
        let diagram = "flowchart LR\n  A --> B\n";
        let source = format!("{diagram}%%{}", " ".repeat(MAX_SOURCE_BYTES - diagram.len() - 2));

        assert_eq!(source.len(), MAX_SOURCE_BYTES);
        assert!(render(&source, WIDTH).is_ok());
    }

    #[test]
    fn a_source_over_the_length_limit_is_refused() {
        let source = format!("flowchart LR\n  A --> B\n%%{}", " ".repeat(MAX_SOURCE_BYTES));

        assert!(matches!(render(&source, WIDTH), Err(Error::TooLong { bytes }) if bytes == source.len()));
    }

    #[test]
    fn a_drawing_at_the_area_limit_is_accepted() {
        assert_eq!(check_area(MAX_AREA / 100, 100), Ok(()));
    }

    #[test]
    fn a_drawing_over_the_area_limit_is_refused() {
        assert_eq!(check_area(MAX_AREA / 100 + 1, 100), Err(Error::TooLarge { width: MAX_AREA / 100 + 1, height: 100 }));
    }

    #[test]
    fn a_sideways_flowchart_too_wide_is_turned_top_down() {
        let source = "flowchart LR\n  A[First step] --> B[Second step] --> C[Third step]";

        let sideways = render(source, WIDTH).unwrap();
        let turned = render(source, 20).unwrap();

        assert_eq!(rows(&sideways.canvas).len(), 3);
        assert!(rows(&turned.canvas).len() > 9);
    }

    #[test]
    fn a_drawing_wider_than_allowed_is_refused() {
        let source = "flowchart LR\n  A[A long first label] --> B[A long second label]";

        assert!(matches!(render(source, 20), Err(Error::TooWide { max_width: 20, .. })));
    }
}
