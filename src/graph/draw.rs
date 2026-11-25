use vt_canvas::{Canvas, Corner, Frame, LineKind, Point, Rect, Role};

use super::Drawing;
use super::diagram::{Diagram, Edge, Head};
use super::route::{Route, opposite};
use super::shape;
use crate::error::Error;

/// Where each part goes on the routing grid.
#[derive(Debug, Clone)]
pub(super) struct Parts {
    pub(super) nodes: Vec<Rect>,
    pub(super) groups: Vec<Rect>,
    /// Where each group's title goes on its frame, a line cell and a space each side of it included.
    pub(super) titles: Vec<Rect>,
    pub(super) routes: Vec<Route>,
    pub(super) labels: Vec<Option<Rect>>,
}

/// Cuts the grid down to what was drawn on it, then draws frames, nodes, edges, titles and labels, in that order:
/// an edge crossing a frame under its title passes behind it.
pub(super) fn draw(diagram: &Diagram, parts: Parts, max_width: u16) -> Result<Drawing, Error> {
    let bounds = bounds(&parts);
    let parts = moved(parts, bounds);
    if bounds.width > max_width {
        return Err(Error::TooWide { width: usize::from(bounds.width), max_width });
    }
    let mut canvas = Canvas::new(bounds.width, bounds.height)
        .map_err(|_canvas_error| Error::TooLarge { width: usize::from(bounds.width), height: usize::from(bounds.height) })?;
    for rect in &parts.groups {
        canvas.frame(*rect, Frame { kind: LineKind::Light, corner: Corner::Rounded, title: None, role: Role::Group });
    }
    for (node, rect) in diagram.nodes.iter().zip(&parts.nodes) {
        shape::draw(&mut canvas, *rect, node.shape, &node.label);
    }
    for (edge, route) in diagram.edges.iter().zip(&parts.routes) {
        draw_edge(&mut canvas, edge, route);
    }
    for (group, at) in diagram.groups.iter().zip(&parts.titles) {
        if let Some(title) = group.title.lines.first().filter(|title| !title.is_empty()) {
            canvas.text_cut(Point::new(at.x + 1, at.y), &format!(" {title} "), group.title.width + 2, Role::Group);
        }
    }
    for (edge, rect) in diagram.edges.iter().zip(&parts.labels) {
        if let (Some(label), Some(rect)) = (&edge.label, rect) {
            canvas.text_cut(Point::new(rect.x, rect.y), &label.lines[0], rect.width, Role::Label);
        }
    }
    Ok(Drawing { canvas, nodes: parts.nodes, groups: parts.groups, routes: parts.routes, labels: parts.labels, wrap: diagram.wrap })
}

/// The line, then its heads: a line meets a node's border only where it draws no head there.
fn draw_edge(canvas: &mut Canvas, edge: &Edge, route: &Route) {
    let mut points = route.corners();
    if edge.end == Head::None {
        points.push(route.end);
    }
    if let (Head::Tips(_), Some(&first_out)) = (edge.start, route.cells.get(1)) {
        points[0] = first_out;
    }
    canvas.path(&points, edge.kind, Role::Edge).unwrap_or_default();
    let ends = [(edge.end, route.cells.iter().rev().copied().collect::<Vec<_>>(), route.arrival())];
    let starts = [(edge.start, route.cells.iter().skip(1).copied().collect::<Vec<_>>(), opposite(route.departure()))];
    for (head, cells, heading) in ends.into_iter().chain(starts) {
        match head {
            Head::None => {}
            Head::Arrow => cells.first().into_iter().for_each(|&cell| canvas.arrow(cell, heading, Role::Edge)),
            Head::Tips(tips) => tips.iter().zip(&cells).for_each(|(&tip, &cell)| canvas.tip(cell, heading, tip, Role::Edge)),
        }
    }
}

/// The smallest rectangle holding every part.
fn bounds(parts: &Parts) -> Rect {
    let rects = parts.nodes.iter().chain(&parts.groups).chain(parts.labels.iter().flatten()).copied();
    let cells = parts.routes.iter().flat_map(|route| route.cells.iter().copied()).map(|cell| Rect::new(cell.x, cell.y, 1, 1));
    let all: Vec<Rect> = rects.chain(cells).collect();
    let left = all.iter().map(|rect| rect.x).min().unwrap_or(0);
    let top = all.iter().map(|rect| rect.y).min().unwrap_or(0);
    let right = all.iter().map(|rect| rect.x + rect.width).max().unwrap_or(0);
    let bottom = all.iter().map(|rect| rect.y + rect.height).max().unwrap_or(0);
    Rect::new(left, top, right - left, bottom - top)
}

fn moved(parts: Parts, bounds: Rect) -> Parts {
    let point = |point: Point| Point::new(point.x - bounds.x, point.y - bounds.y);
    let rect = |rect: Rect| Rect::new(rect.x - bounds.x, rect.y - bounds.y, rect.width, rect.height);
    Parts {
        nodes: parts.nodes.into_iter().map(rect).collect(),
        groups: parts.groups.into_iter().map(rect).collect(),
        titles: parts.titles.into_iter().map(rect).collect(),
        routes: parts
            .routes
            .into_iter()
            .map(|route| Route { cells: route.cells.into_iter().map(point).collect(), end: point(route.end) })
            .collect(),
        labels: parts.labels.into_iter().map(|label| label.map(rect)).collect(),
    }
}
