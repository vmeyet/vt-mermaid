use vt_canvas::{Canvas, Corner, Frame, LineKind, Point, Rect, Role};

use super::label::Label;

/// How a node's border is drawn; Mermaid shapes without a text form of their own are boxes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Shape {
    Box,
    Rounded,
    /// A box with `╱ ╲` corners.
    Diamond,
    /// A rounded box with a line under its top.
    Cylinder,
    /// A box with an inner line down each side.
    Subroutine,
}

impl Shape {
    /// Columns and rows of the node, border included, so the label has a space on each side.
    pub(super) fn size(self, label: &Label) -> Size {
        let inset = self.inset();
        Size { width: label.width + 2 * inset.width, height: label.height() + inset.height + 1 }
    }

    pub(super) fn draw(self, canvas: &mut Canvas, rect: Rect, label: &Label) {
        let corner = if matches!(self, Self::Rounded | Self::Cylinder) { Corner::Rounded } else { Corner::Square };
        canvas.frame(rect, Frame { kind: LineKind::Light, corner, title: None, role: Role::Node });
        let (left, top, right, bottom) = (rect.x, rect.y, rect.x + rect.width - 1, rect.y + rect.height - 1);
        match self {
            Self::Box | Self::Rounded => {}
            Self::Diamond => {
                for (x, y, corner) in [(left, top, "╱"), (right, top, "╲"), (left, bottom, "╲"), (right, bottom, "╱")] {
                    canvas.text(Point::new(x, y), corner, Role::Node);
                }
            }
            Self::Cylinder => line(canvas, Point::new(left, top + 1), Point::new(right, top + 1)),
            Self::Subroutine => {
                line(canvas, Point::new(left + 1, top), Point::new(left + 1, bottom));
                line(canvas, Point::new(right - 1, top), Point::new(right - 1, bottom));
            }
        }
        draw_label(canvas, rect, self.inset(), label);
    }

    /// Columns from the left border to the label, rows from the top border to the label.
    fn inset(self) -> Size {
        match self {
            Self::Box | Self::Rounded | Self::Diamond => Size { width: 2, height: 1 },
            Self::Cylinder => Size { width: 2, height: 2 },
            Self::Subroutine => Size { width: 3, height: 1 },
        }
    }
}

/// Columns and rows a block covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Size {
    pub(super) width: u16,
    pub(super) height: u16,
}

fn line(canvas: &mut Canvas, from: Point, to: Point) {
    canvas.path(&[from, to], LineKind::Light, Role::Node).unwrap_or_default();
}

/// Each line centred in the space between the insets.
fn draw_label(canvas: &mut Canvas, rect: Rect, inset: Size, label: &Label) {
    for (row, text) in (rect.y + inset.height..).zip(&label.lines) {
        let text_width = u16::try_from(vt_canvas::width(text)).unwrap_or(u16::MAX).min(label.width);
        let x = rect.x + inset.width + (label.width - text_width) / 2;
        canvas.text_cut(Point::new(x, row), text, label.width, Role::Label);
    }
}

#[cfg(test)]
mod tests {
    use vt_canvas::GlyphSet;

    use super::*;

    const SHAPES: [Shape; 5] = [Shape::Box, Shape::Rounded, Shape::Diamond, Shape::Cylinder, Shape::Subroutine];

    #[test]
    fn each_shape_leaves_a_space_on_each_side_of_its_label() {
        let label = Label::wrapped("ab");

        let sizes: Vec<(u16, u16)> = SHAPES.iter().map(|shape| shape.size(&label)).map(|size| (size.width, size.height)).collect();

        assert_eq!(sizes, [(6, 3), (6, 3), (6, 3), (6, 4), (8, 3)]);
    }

    #[test]
    fn each_shape_draws_its_border_around_its_centred_label() {
        let label = Label::wrapped("Cached?<br>yes");
        let mut canvas = Canvas::new(70, 5).unwrap();

        for (x, shape) in (0..).step_by(14).zip(SHAPES) {
            let size = shape.size(&label);
            shape.draw(&mut canvas, Rect::new(x, 0, size.width, size.height), &label);
        }

        let rows: Vec<String> = canvas.rows(GlyphSet::Unicode).map(|runs| runs.into_iter().map(|run| run.text).collect()).collect();
        insta::assert_snapshot!(rows.join("\n"));
    }

    #[test]
    fn a_label_line_wider_than_the_label_is_cut() {
        let label = Label::wrapped("state-diagram-new-arrow-florbs-and-more");
        let size = Shape::Box.size(&label);
        let mut canvas = Canvas::new(size.width, size.height).unwrap();

        Shape::Box.draw(&mut canvas, Rect::new(0, 0, size.width, size.height), &label);

        let middle: String = canvas.rows(GlyphSet::Unicode).nth(1).unwrap().into_iter().map(|run| run.text).collect();
        assert_eq!(middle, "│ state-diagram-new-arrow-florb⋯ │");
    }
}
