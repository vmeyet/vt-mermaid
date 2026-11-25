//! Mermaid's flowchart shape names, each drawn as the closest vt-canvas shape.

use vt_canvas::Shape;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shape_name_maps_to_the_closest_shape_and_an_unknown_one_to_a_rectangle() {
        let shapes = [Some("round"), Some("trap-t"), Some("database"), Some("cloud"), None].map(named);

        assert_eq!(shapes, [Shape::Rounded, Shape::InvertedTrapezoid, Shape::Cylinder, Shape::Rectangle, Shape::Rectangle]);
    }
}
