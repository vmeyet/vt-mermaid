use vt_canvas::{Canvas, Point, Rect, Role, Shape};

use crate::label::Label;

/// Mermaid's shape names, from brackets or a block, each drawn as the closest vt-canvas shape; any other name is a
/// rectangle.
const SHAPES: [(&str, Shape); 57] = [
    ("round", Shape::Rounded),
    ("ellipse", Shape::Circle),
    ("doublecircle", Shape::DoubleCircle),
    ("lean_right", Shape::LeanRight),
    ("lean_left", Shape::LeanLeft),
    ("rounded", Shape::Rounded),
    ("event", Shape::Rounded),
    ("delay", Shape::Rounded),
    ("half-rounded-rectangle", Shape::Rounded),
    ("stadium", Shape::Stadium),
    ("pill", Shape::Stadium),
    ("terminal", Shape::Stadium),
    ("bow-rect", Shape::Stadium),
    ("bow-tie-rectangle", Shape::Stadium),
    ("stored-data", Shape::Stadium),
    ("circle", Shape::Circle),
    ("circ", Shape::Circle),
    ("sm-circ", Shape::Circle),
    ("small-circle", Shape::Circle),
    ("start", Shape::Circle),
    ("f-circ", Shape::Circle),
    ("filled-circle", Shape::Circle),
    ("junction", Shape::Circle),
    ("cross-circ", Shape::Circle),
    ("crossed-circle", Shape::Circle),
    ("summary", Shape::Circle),
    ("dbl-circ", Shape::DoubleCircle),
    ("double-circle", Shape::DoubleCircle),
    ("fr-circ", Shape::DoubleCircle),
    ("framed-circle", Shape::DoubleCircle),
    ("stop", Shape::DoubleCircle),
    ("subroutine", Shape::Subroutine),
    ("subproc", Shape::Subroutine),
    ("subprocess", Shape::Subroutine),
    ("fr-rect", Shape::Subroutine),
    ("framed-rectangle", Shape::Subroutine),
    ("cyl", Shape::Cylinder),
    ("cylinder", Shape::Cylinder),
    ("database", Shape::Cylinder),
    ("db", Shape::Cylinder),
    ("lin-cyl", Shape::Cylinder),
    ("lined-cylinder", Shape::Cylinder),
    ("disk", Shape::Cylinder),
    ("diam", Shape::Diamond),
    ("diamond", Shape::Diamond),
    ("decision", Shape::Diamond),
    ("question", Shape::Diamond),
    ("hex", Shape::Hexagon),
    ("hexagon", Shape::Hexagon),
    ("prepare", Shape::Hexagon),
    ("lean-r", Shape::LeanRight),
    ("lean-right", Shape::LeanRight),
    ("in-out", Shape::LeanRight),
    ("lean-l", Shape::LeanLeft),
    ("lean-left", Shape::LeanLeft),
    ("out-in", Shape::LeanLeft),
    ("odd", Shape::Asymmetric),
];

/// Mermaid's trapezoid names: `b` is wide at the bottom, `t` at the top.
const TRAPEZOIDS: [(&str, Shape); 13] = [
    ("trap-b", Shape::Trapezoid),
    ("trapezoid", Shape::Trapezoid),
    ("trapezoid-bottom", Shape::Trapezoid),
    ("priority", Shape::Trapezoid),
    ("tri", Shape::Trapezoid),
    ("triangle", Shape::Trapezoid),
    ("extract", Shape::Trapezoid),
    ("trap-t", Shape::InvertedTrapezoid),
    ("inv_trapezoid", Shape::InvertedTrapezoid),
    ("trapezoid-top", Shape::InvertedTrapezoid),
    ("inv-trapezoid", Shape::InvertedTrapezoid),
    ("manual", Shape::InvertedTrapezoid),
    ("flip-tri", Shape::InvertedTrapezoid),
];

/// The shape mermaid names `name`, a rectangle for a plain node.
pub(super) fn named(name: Option<&str>) -> Shape {
    let known = SHAPES.iter().chain(&TRAPEZOIDS).find(|(known, _)| Some(*known) == name);
    known.map_or(Shape::Rectangle, |&(_, shape)| shape)
}

/// Columns and rows a block covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Size {
    pub(super) width: u16,
    pub(super) height: u16,
}

/// Columns and rows of a node, its outline included, so the label has a space on each side.
pub(super) fn size(shape: Shape, label: &Label) -> Size {
    let inset = shape.inset();
    Size { width: label.width + 2 * (inset.side + 1), height: label.height() + inset.top + inset.bottom }
}

/// The outline, then each line of the label centred inside it.
pub(super) fn draw(canvas: &mut Canvas, rect: Rect, shape: Shape, label: &Label) {
    canvas.shape(rect, shape, Role::Node);
    let inset = shape.inset();
    for (row, text) in (rect.y + inset.top..).zip(&label.lines) {
        let text_width = u16::try_from(vt_canvas::width(text)).unwrap_or(u16::MAX).min(label.width);
        let x = rect.x + inset.side + 1 + (label.width - text_width) / 2;
        canvas.text_cut(Point::new(x, row), text, label.width, Role::Label);
    }
}

#[cfg(test)]
mod tests {
    use vt_canvas::GlyphSet;

    use super::*;

    fn drawn(shape: Shape, label: &Label) -> Vec<String> {
        let size = size(shape, label);
        let mut canvas = Canvas::new(size.width, size.height).unwrap();
        draw(&mut canvas, Rect::new(0, 0, size.width, size.height), shape, label);
        canvas.rows(GlyphSet::Unicode).map(|runs| runs.into_iter().map(|run| run.text).collect()).collect()
    }

    #[test]
    fn a_shape_leaves_a_space_between_its_outline_and_its_label() {
        let label = Label::wrapped("ab");

        let sizes = [Shape::Rectangle, Shape::Subroutine, Shape::Cylinder].map(|shape| size(shape, &label));

        assert_eq!(sizes, [Size { width: 6, height: 3 }, Size { width: 8, height: 3 }, Size { width: 6, height: 4 }]);
    }

    #[test]
    fn each_shape_draws_its_outline_around_its_centred_label() {
        let label = Label::wrapped("Cached?<br>yes");

        let drawn: Vec<String> = Shape::ALL.iter().map(|&shape| drawn(shape, &label).join("\n")).collect();

        insta::assert_snapshot!(drawn.join("\n\n"));
    }

    #[test]
    fn a_shape_name_maps_to_the_closest_shape_and_an_unknown_one_to_a_rectangle() {
        let shapes = [Some("round"), Some("trap-t"), Some("database"), Some("cloud"), None].map(named);

        assert_eq!(shapes, [Shape::Rounded, Shape::InvertedTrapezoid, Shape::Cylinder, Shape::Rectangle, Shape::Rectangle]);
    }

    #[test]
    fn a_label_line_wider_than_the_label_is_cut() {
        let label = Label { lines: vec!["state-diagram-new-arrow-florbs-and-more".to_owned()], width: 30 };

        assert_eq!(drawn(Shape::Rectangle, &label)[1], "│ state-diagram-new-arrow-florb⋯ │");
    }
}
