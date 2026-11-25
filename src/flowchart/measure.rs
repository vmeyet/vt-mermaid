//! Exact measures of a drawing, read from where each part went rather than from its glyphs.
//!
//! Built for the scoreboard and the level C test only, behind the `measure` feature: no caller needs them.

use std::collections::HashSet;

use vt_canvas::{Canvas, GlyphSet, Point, Rect};

use super::diagram::{self, Diagram};
use super::route::Route;
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
    /// Cells an edge runs through right beside the border of a node it does not join.
    pub beside_nodes: usize,
    /// Level C invariants the drawing breaks, one line each.
    pub violations: Vec<String>,
}

/// Draws `source` as [`render`](crate::render) does, then measures the drawing.
pub fn measure(source: &str, max_width: u16) -> Result<Measures, Error> {
    let drawing = render(source, max_width)?;
    let diagram = Diagram::read(source)?.wrapped(drawing.wrap);
    Ok(Measures {
        crossings: crossings(&drawing),
        bends: bends(&drawing),
        edge_cells: drawing.routes.iter().flat_map(|route| &route.cells).collect::<HashSet<_>>().len(),
        beside_nodes: beside_nodes(&diagram, &drawing),
        violations: violations(&diagram, &drawing),
    })
}

/// Cells one route runs straight through sideways and another up or down: routes sharing a trunk run the same way.
fn crossings(drawing: &Drawing) -> usize {
    let straight = |sideways: bool| -> HashSet<Point> {
        drawing
            .routes
            .iter()
            .flat_map(Route::straight)
            .filter(|&(_, runs_sideways)| runs_sideways == sideways)
            .map(|(cell, _)| cell)
            .collect()
    };
    straight(true).intersection(&straight(false)).count()
}

/// Turns along every route, one shared by the routes of a trunk counted once.
fn bends(drawing: &Drawing) -> usize {
    let turns = |route: &Route| {
        route
            .cells
            .windows(3)
            .filter(|cells| (cells[0].x == cells[1].x) != (cells[1].x == cells[2].x))
            .map(<[Point]>::to_vec)
            .collect::<Vec<_>>()
    };
    drawing.routes.iter().flat_map(turns).collect::<HashSet<_>>().len()
}

/// Cells a route runs straight through with a node it does not join right beside it: its line reads as a second
/// border of that node.
fn beside_nodes(diagram: &Diagram, drawing: &Drawing) -> usize {
    let rect = |end| match end {
        diagram::End::Node(index) => drawing.nodes[index],
        diagram::End::Group(index) => drawing.groups[index],
    };
    let beside = |(cell, sideways): (Point, bool)| {
        let (before, after) = if sideways { ((0, -1), (0, 1)) } else { ((-1, 0), (1, 0)) };
        [before, after].into_iter().filter_map(move |(x, y)| Some(Point::new(cell.x.checked_add_signed(x)?, cell.y.checked_add_signed(y)?)))
    };
    let mut cells = HashSet::new();
    for (edge, route) in diagram.edges.iter().zip(&drawing.routes) {
        let ends = [rect(edge.from), rect(edge.to)];
        let hugs = |near: Point| drawing.nodes.iter().any(|node| contains(*node, near) && !ends.contains(node));
        cells.extend(route.straight().filter(|&run| beside(run).any(hugs)).map(|(cell, _)| cell));
    }
    cells.len()
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
        assert_eq!(measured("tests/spike/cached.mmd").crossings, 1);
    }

    #[test]
    fn an_edge_crossing_a_frame_is_not_a_crossing() {
        assert_eq!(measured("corpus/flowchart/docs--config-theming--L155.mmd").crossings, 0);
    }

    #[test]
    fn a_straight_edge_has_no_bend_and_each_edge_round_two_corners_has_two() {
        let straight = measure("flowchart TB\n  A --> B", 80).unwrap();
        let cornered = measure("flowchart TB\n  A --> B & C", 80).unwrap();

        assert_eq!((straight.bends, straight.edge_cells, straight.violations.len()), (0, 4, 0));
        assert_eq!(cornered.bends, 4);
    }

    #[test]
    fn an_edge_running_beside_a_node_it_does_not_join_counts_each_cell_beside_it() {
        let apart = measure("flowchart TB\n  A --> B", 80).unwrap();
        let beside = measure("flowchart TB\n  A & B --> C & D", 80).unwrap();

        assert_eq!((apart.beside_nodes, beside.beside_nodes), (0, 2));
    }
}
