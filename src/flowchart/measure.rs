//! Exact measures of a drawing, read from where each part went rather than from its glyphs.
//!
//! Built for the scoreboard and the level C test only, behind the `measure` feature: no caller needs them.

use std::collections::HashMap;

use vt_canvas::{Canvas, GlyphSet, Point, Rect};

use super::diagram::{self, Diagram};
use super::{Drawing, render};
use crate::error::Error;

/// How a drawing is laid out, counted on its own geometry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Measures {
    /// Places where two edges cross.
    pub crossings: usize,
    /// Turns along all edges, their ends left out.
    pub bends: usize,
    /// Cells all edges cover.
    pub edge_cells: usize,
    /// Level C invariants the drawing breaks, one line each.
    pub violations: Vec<String>,
}

/// Draws `source` as [`render`](crate::render) does, then measures the drawing.
pub fn measure(source: &str, max_width: u16) -> Result<Measures, Error> {
    let drawing = render(source, max_width)?;
    let diagram = Diagram::read(source)?;
    Ok(Measures {
        crossings: crossings(&drawing),
        bends: drawing.routes.iter().map(super::route::Route::bends).sum(),
        edge_cells: drawing.routes.iter().map(|route| route.cells.len()).sum(),
        violations: violations(&diagram, &drawing),
    })
}

/// Lines never run along each other, so a cell two routes share is a crossing.
fn crossings(drawing: &Drawing) -> usize {
    let mut routes_by_cell: HashMap<Point, usize> = HashMap::new();
    for cell in drawing.routes.iter().flat_map(|route| &route.cells) {
        *routes_by_cell.entry(*cell).or_default() += 1;
    }
    routes_by_cell.values().map(|&routes| routes * (routes - 1) / 2).sum()
}

pub(super) fn rows(canvas: &Canvas) -> Vec<String> {
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
pub(super) fn violations(diagram: &Diagram, drawing: &Drawing) -> Vec<String> {
    let rows = rows(&drawing.canvas);
    let rect = |end| match end {
        diagram::End::Node(index) => drawing.nodes[index],
        diagram::End::Group(index) => drawing.groups[index],
    };
    let mut found = Vec::new();
    for (index, (node, &node_rect)) in diagram.nodes.iter().zip(&drawing.nodes).enumerate() {
        let drawn: String =
            (node_rect.y..node_rect.y + node_rect.height).map(|y| text_at(&rows, Rect::new(node_rect.x, y, node_rect.width, 1))).collect();
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
        let (Some(&first), Some(&last)) = (route.cells.first(), route.cells.last()) else {
            found.push(format!("{name} has no cells"));
            continue;
        };
        let interior = Rect::new(from.x + 1, from.y + 1, from.width - 2, from.height - 2);
        if !contains(from, first) || contains(interior, first) {
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;

    fn measured(path: &str) -> Measures {
        let source = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap();
        measure(&source, 400).unwrap()
    }

    #[test]
    fn edges_crossing_each_other_are_counted_once_each() {
        assert_eq!(measured("tests/spike/cached.mmd").crossings, 4);
    }

    #[test]
    fn an_edge_crossing_a_frame_is_not_a_crossing() {
        assert_eq!(measured("corpus/flowchart/docs--config-theming--L155.mmd").crossings, 0);
    }

    #[test]
    fn a_straight_edge_has_no_bend_and_an_edge_round_a_corner_has_one() {
        let straight = measure("flowchart TB\n  A --> B", 80).unwrap();
        let cornered = measure("flowchart TB\n  A --> B & C", 80).unwrap();

        assert_eq!((straight.bends, straight.edge_cells, straight.violations.len()), (0, 4, 0));
        assert_eq!(cornered.bends, 2);
    }
}
