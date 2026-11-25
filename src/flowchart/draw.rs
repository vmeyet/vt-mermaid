use vt_canvas::{Canvas, Corner, Frame, LineKind, Point, Rect, Role};

use super::Drawing;
use super::diagram::Diagram;
use super::route::{Route, opposite};
use super::shape;
use crate::error::Error;

/// Where each part goes on the routing grid.
#[derive(Debug, Clone)]
pub(super) struct Parts {
    pub(super) nodes: Vec<Rect>,
    pub(super) groups: Vec<Rect>,
    pub(super) routes: Vec<Route>,
    pub(super) labels: Vec<Option<Rect>>,
}

/// Cuts the grid down to what was drawn on it, then draws groups, nodes, edges and labels, in that order.
pub(super) fn draw(diagram: &Diagram, parts: Parts, max_width: u16) -> Result<Drawing, Error> {
    let bounds = bounds(&parts);
    let parts = moved(parts, bounds);
    if bounds.width > max_width {
        return Err(Error::TooWide { width: usize::from(bounds.width), max_width });
    }
    let mut canvas = Canvas::new(bounds.width, bounds.height)
        .map_err(|_canvas_error| Error::TooLarge { width: usize::from(bounds.width), height: usize::from(bounds.height) })?;
    for (group, rect) in diagram.groups.iter().zip(&parts.groups) {
        let title = group.title.lines.first().map(String::as_str).filter(|title| !title.is_empty());
        canvas.frame(*rect, Frame { kind: LineKind::Light, corner: Corner::Rounded, title, role: Role::Group });
    }
    for (node, rect) in diagram.nodes.iter().zip(&parts.nodes) {
        shape::draw(&mut canvas, *rect, node.shape, &node.label);
    }
    for (edge, route) in diagram.edges.iter().zip(&parts.routes) {
        let mut points = route.corners();
        if !edge.arrow_end {
            points.push(route.end);
        }
        canvas.path(&points, edge.kind, Role::Edge).unwrap_or_default();
        if edge.arrow_end
            && let Some(&last) = route.cells.last()
        {
            canvas.arrow(last, route.arrival(), Role::Edge);
        }
        if edge.arrow_start
            && let Some(&first_out) = route.cells.get(1)
        {
            canvas.arrow(first_out, opposite(route.departure()), Role::Edge);
        }
    }
    for (edge, rect) in diagram.edges.iter().zip(&parts.labels) {
        if let (Some(label), Some(rect)) = (&edge.label, rect) {
            canvas.text_cut(Point::new(rect.x, rect.y), &label.lines[0], rect.width, Role::Label);
        }
    }
    Ok(Drawing { canvas, nodes: parts.nodes, groups: parts.groups, routes: parts.routes, labels: parts.labels })
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
        routes: parts
            .routes
            .into_iter()
            .map(|route| Route { cells: route.cells.into_iter().map(point).collect(), end: point(route.end) })
            .collect(),
        labels: parts.labels.into_iter().map(|label| label.map(rect)).collect(),
    }
}
