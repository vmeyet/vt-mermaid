use vt_canvas::{Canvas, LineKind, Point, Rect, Role, Shape};

use crate::label::Label;

/// Columns and rows a block covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Size {
    pub(crate) width: u16,
    pub(crate) height: u16,
}

/// Columns and rows of a node, its outline included, so its text has a space on each side; each compartment adds
/// its rule and its lines.
pub(super) fn size(shape: Shape, label: &Label, compartments: &[Vec<String>]) -> Size {
    let inset = shape.inset();
    let rows: usize = compartments.iter().map(|lines| 1 + lines.len()).sum();
    Size {
        width: inner_width(label, compartments) + 2 * (inset.side + 1),
        height: label.height() + to_u16(rows) + inset.top + inset.bottom,
    }
}

/// The outline, each line of the label centred inside it, then each compartment under a rule, its lines from the left.
pub(super) fn draw(canvas: &mut Canvas, rect: Rect, shape: Shape, label: &Label, compartments: &[Vec<String>]) {
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

    fn drawn(shape: Shape, label: &Label) -> Vec<String> {
        drawn_with(shape, label, &[])
    }

    fn drawn_with(shape: Shape, label: &Label, compartments: &[Vec<String>]) -> Vec<String> {
        let size = size(shape, label, compartments);
        let mut canvas = Canvas::new(size.width, size.height).unwrap();
        draw(&mut canvas, Rect::new(0, 0, size.width, size.height), shape, label, compartments);
        canvas.rows(GlyphSet::Unicode).map(|runs| runs.into_iter().map(|run| run.text).collect()).collect()
    }

    #[test]
    fn a_shape_leaves_a_space_between_its_outline_and_its_label() {
        let label = Label::wrapped("ab");

        let sizes = [Shape::Rectangle, Shape::Subroutine, Shape::Cylinder].map(|shape| size(shape, &label, &[]));

        assert_eq!(sizes, [Size { width: 6, height: 3 }, Size { width: 8, height: 3 }, Size { width: 6, height: 4 }]);
    }

    #[test]
    fn each_shape_draws_its_outline_around_its_centred_label() {
        let label = Label::wrapped("Cached?<br>yes");

        let drawn: Vec<String> = Shape::ALL.iter().map(|&shape| drawn(shape, &label).join("\n")).collect();

        insta::assert_snapshot!(drawn.join("\n\n"));
    }

    #[test]
    fn a_label_line_wider_than_the_label_is_cut() {
        let label = Label { lines: vec!["state-diagram-new-arrow-florbs-and-more".to_owned()], width: 30 };

        assert_eq!(drawn(Shape::Rectangle, &label)[1], "│ state-diagram-new-arrow-florb⋯ │");
    }

    #[test]
    fn compartments_stand_under_rules_their_lines_from_the_left() {
        let compartments = [vec!["+int age".to_owned()], vec!["+swim()".to_owned(), "+quack() bool".to_owned()]];

        let rows = drawn_with(Shape::Rectangle, &Label::wrapped("Duck"), &compartments);

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
