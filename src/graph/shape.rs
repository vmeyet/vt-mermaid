use vt_canvas::{Canvas, Point, Rect, Role, Shape};

use crate::label::Label;

/// Columns and rows a block covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Size {
    pub(crate) width: u16,
    pub(crate) height: u16,
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
    fn a_label_line_wider_than_the_label_is_cut() {
        let label = Label { lines: vec!["state-diagram-new-arrow-florbs-and-more".to_owned()], width: 30 };

        assert_eq!(drawn(Shape::Rectangle, &label)[1], "│ state-diagram-new-arrow-florb⋯ │");
    }
}
