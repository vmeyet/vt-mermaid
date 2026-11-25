//! Where everything goes, in cells: each participant's column from the room the texts beside it need, then each event
//! on rows of its own, top to bottom.

mod columns;

use super::diagram::{Event, Figure};
use crate::label::{Label, Wrap};

/// Spaces between two participants' heads, loose then tight.
pub(super) const LOOSE: i32 = 3;
pub(super) const TIGHT: i32 = 1;
/// Columns a block's frame keeps on each side of what it holds, for each level of block inside it.
pub(super) const FRAME_PADDING: i32 = 2;
/// Columns between a box of participants and the heads it holds.
const GROUP_PADDING: i32 = 2;

/// How tight a layout is: how labels wrap and the spaces between heads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Fit {
    pub(super) wrap: Wrap,
    pub(super) gap: i32,
}

/// Cells from `left` to `right` and `top` to `bottom`, all included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Area {
    pub(super) left: i32,
    pub(super) top: i32,
    pub(super) right: i32,
    pub(super) bottom: i32,
}

impl Area {
    fn sized(left: i32, top: i32, width: i32, height: i32) -> Self {
        Self { left, top, right: left + width - 1, bottom: top + height - 1 }
    }

    pub(super) fn union(self, other: Self) -> Self {
        Self {
            left: self.left.min(other.left),
            top: self.top.min(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Head {
    pub(super) area: Area,
    pub(super) figure: Figure,
    pub(super) label: Label,
}

/// A head sized for its label, at the origin.
fn head(figure: Figure, label: Label) -> Head {
    let (width, height) = match figure {
        Figure::Box => (i32::from(label.width) + 4, i32::from(label.height()) + 2),
        Figure::Person => (i32::from(label.width).max(3), i32::from(label.height()) + 3),
        Figure::Database => (i32::from(label.width) + 4, i32::from(label.height()) + 3),
    };
    Head { area: Area::sized(0, 0, width, height), figure, label }
}

fn event_label(event: &Event, wrap: Wrap) -> Label {
    match event {
        Event::Message { text, .. } | Event::Note { text, .. } => Label::wrapped_as(text, wrap),
        _ => Label::wrapped_as("", wrap),
    }
}

impl Head {
    fn width(&self) -> i32 {
        self.area.right - self.area.left + 1
    }

    fn height(&self) -> i32 {
        self.area.bottom - self.area.top + 1
    }

    /// Columns left of the lifeline, which runs down the middle, and right of it.
    pub(super) fn before(&self) -> i32 {
        (self.width() - 1) / 2
    }

    pub(super) fn after(&self) -> i32 {
        self.width() - 1 - self.before()
    }
}
