use vt_canvas::{Canvas, LineKind, Point, Rect, Role, Shape};

use super::diagram::Figure;
use crate::label::Label;

/// Columns and rows a block covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Size {
    pub(crate) width: u16,
    pub(crate) height: u16,
}

/// Columns and rows of a node, its outline included, so its text has a space on each side; each compartment adds
/// its rule and its lines; a bar lies across the flow, going down or up when `vertical`.
pub(super) fn size(figure: Figure, label: &Label, compartments: &[Vec<String>], vertical: bool) -> Size {
    match figure {
        Figure::Shape(shape) => outlined(shape, label, compartments),
        Figure::Marker(_) => Size { width: 1, height: 1 },
        Figure::Bar { length } if vertical => Size { width: length, height: 1 },
        Figure::Bar { length } => Size { width: 1, height: length },
    }
}

fn outlined(shape: Shape, label: &Label, compartments: &[Vec<String>]) -> Size {
    let inset = shape.inset();
    let rows: usize = compartments.iter().map(|lines| 1 + lines.len()).sum();
    Size {
        width: inner_width(label, compartments) + 2 * (inset.side + 1),
        height: label.height() + to_u16(rows) + inset.top + inset.bottom,
    }
}

/// The figure on `rect`, with its label and compartments when it shows them.
pub(super) fn draw(canvas: &mut Canvas, rect: Rect, figure: Figure, label: &Label, compartments: &[Vec<String>]) {
    match figure {
        Figure::Shape(shape) => draw_outlined(canvas, rect, shape, label, compartments),
        Figure::Marker(marker) => canvas.marker(Point::new(rect.x, rect.y), marker, Role::Node),
        Figure::Bar { .. } => {
            let end = Point::new(rect.x + rect.width.saturating_sub(1), rect.y + rect.height.saturating_sub(1));
            canvas.line(Point::new(rect.x, rect.y), end, LineKind::Heavy, Role::Node).unwrap_or_default();
        }
    }
}

/// The outline, each line of the label centred inside it, then each compartment under a rule, its lines from the left.
fn draw_outlined(canvas: &mut Canvas, rect: Rect, shape: Shape, label: &Label, compartments: &[Vec<String>]) {
    canvas.shape(rect, shape, Role::Node);
    let inset = shape.inset();
    let (left, width) = (rect.x + inset.side + 1, inner_width(label, compartments));
    for (row, text) in (rect.y + inset.top..).zip(&label.lines) {
        let text_width = to_u16(vt_canvas::width(text)).min(width);
        canvas.text_cut(Point::new(left + (width - text_width) / 2, row), text, width, Role::Label);
    }
    let mut row = rect.y + inset.top + label.height();
    for lines in compartments {
        let rule = (Point::new(rect.x, row), Point::new(rect.x + rect.width.saturating_sub(1), row));
        canvas.line(rule.0, rule.1, LineKind::Light, Role::Node).unwrap_or_default();
        for (line_row, text) in (row + 1..).zip(lines) {
            canvas.text_cut(Point::new(left, line_row), text, width, Role::Label);
        }
        row += 1 + to_u16(lines.len());
    }
}

/// Columns between the spaces inside the outline: the widest of the label and the compartments' lines.
fn inner_width(label: &Label, compartments: &[Vec<String>]) -> u16 {
    let widest = compartments.iter().flatten().map(|line| to_u16(vt_canvas::width(line))).max().unwrap_or(0);
    label.width.max(widest)
}

fn to_u16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    use vt_canvas::GlyphSet;

    use super::*;

    fn drawn(figure: Figure, label: &Label) -> Vec<String> {
        drawn_with(figure, label, &[])
    }

    fn drawn_with(figure: Figure, label: &Label, compartments: &[Vec<String>]) -> Vec<String> {
        let size = size(figure, label, compartments, true);
        let mut canvas = Canvas::new(size.width, size.height).unwrap();
        draw(&mut canvas, Rect::new(0, 0, size.width, size.height), figure, label, compartments);
        canvas.rows(GlyphSet::Unicode).map(|runs| runs.into_iter().map(|run| run.text).collect()).collect()
    }

    #[test]
    fn a_shape_leaves_a_space_between_its_outline_and_its_label() {
        let label = Label::wrapped("ab");

        let sizes = [Shape::Rectangle, Shape::Subroutine, Shape::Cylinder].map(|shape| size(Figure::Shape(shape), &label, &[], true));

        assert_eq!(sizes, [Size { width: 6, height: 3 }, Size { width: 8, height: 3 }, Size { width: 6, height: 4 }]);
    }

    #[test]
    fn each_shape_draws_its_outline_around_its_centred_label() {
        let label = Label::wrapped("Cached?<br>yes");

        let drawn: Vec<String> = Shape::ALL.iter().map(|&shape| drawn(Figure::Shape(shape), &label).join("\n")).collect();

        insta::assert_snapshot!(drawn.join("\n\n"));
    }

    #[test]
    fn a_label_line_wider_than_the_label_is_cut() {
        let label = Label { lines: vec!["state-diagram-new-arrow-florbs-and-more".to_owned()], width: 30 };

        assert_eq!(drawn(Figure::Shape(Shape::Rectangle), &label)[1], "│ state-diagram-new-arrow-florb⋯ │");
    }

    #[test]
    fn a_marker_takes_one_cell_and_a_bar_lies_across_the_flow() {
        let marker = Figure::Marker(vt_canvas::Marker::Dot);
        let bar = Figure::Bar { length: 5 };

        let sizes =
            [size(marker, &Label::default(), &[], true), size(bar, &Label::default(), &[], true), size(bar, &Label::default(), &[], false)];

        assert_eq!(sizes, [Size { width: 1, height: 1 }, Size { width: 5, height: 1 }, Size { width: 1, height: 5 }]);
        assert_eq!(drawn(bar, &Label::default()), ["━━━━━"]);
    }

    #[test]
    fn compartments_stand_under_rules_their_lines_from_the_left() {
        let compartments = [vec!["+int age".to_owned()], vec!["+swim()".to_owned(), "+quack() bool".to_owned()]];

        let rows = drawn_with(Figure::Shape(Shape::Rectangle), &Label::wrapped("Duck"), &compartments);

        assert_eq!(
            rows,
            [
                "┌───────────────┐",
                "│     Duck      │",
                "├───────────────┤",
                "│ +int age      │",
                "├───────────────┤",
                "│ +swim()       │",
                "│ +quack() bool │",
                "└───────────────┘"
            ]
        );
    }
}
