//! Draws a placement on a canvas: lifelines, boxes of participants, block frames, heads, notes, then messages.
//!
//! A message, a frame's border or a note passes in front of a lifeline: the lifeline stops there instead of crossing.

use vt_canvas::{Canvas, Corner, Direction, Frame, LineKind, Point, Rect, Role, Shape};

use super::diagram::Figure;
use super::layout::{Area, Cell, Frame as Block, Head, Layout, Lifeline, Message};
use super::parse::Tip;
use crate::error::Error;
use crate::label::Label;

/// The stick figure of an `actor`, over its name.
const PERSON: [&str; 3] = [" o ", "/|\\", "/ \\"];

/// Draws `layout`, refused when wider than `max_width`.
pub(super) fn draw(layout: &Layout, max_width: u16) -> Result<Canvas, Error> {
    let bounds = bounds(layout);
    let (width, height) = (bounds.right - bounds.left + 1, bounds.bottom - bounds.top + 1);
    if width > i32::from(max_width) {
        return Err(Error::TooWide { width: usize::try_from(width).unwrap_or(usize::MAX), max_width });
    }
    let too_large =
        || Error::TooLarge { width: usize::try_from(width).unwrap_or(usize::MAX), height: usize::try_from(height).unwrap_or(usize::MAX) };
    let size = |value: i32| u16::try_from(value).map_err(|_overflow| too_large());
    let canvas = Canvas::new(size(width)?, size(height)?).map_err(|_canvas_error| too_large())?;
    let mut pen = Pen { canvas, origin: (bounds.left, bounds.top) };
    for lifeline in &layout.lifelines {
        pen.lifeline(lifeline);
    }
    for (x, y) in crossings(layout) {
        pen.text(x, y, " ", Role::Plain);
    }
    for group in &layout.groups {
        pen.frame(group.area, LineKind::Light, Corner::Rounded, group.title.as_deref());
    }
    for block in &layout.frames {
        pen.block(block);
    }
    for head in &layout.heads {
        pen.head(head);
    }
    for note in &layout.notes {
        pen.frame(note.area, LineKind::Light, Corner::Rounded, None);
        pen.centred(&note.label, note.area.left + 2, note.area.top + 1, note.area.right - note.area.left - 3, Role::Label);
    }
    for message in &layout.messages {
        pen.message(message);
    }
    for cross in &layout.crosses {
        pen.canvas.cross(pen.point(cross.x, cross.y), Role::Edge);
    }
    Ok(pen.canvas)
}

/// The canvas and where the placement's origin falls on it.
#[derive(Debug)]
struct Pen {
    canvas: Canvas,
    origin: (i32, i32),
}

impl Pen {
    fn point(&self, x: i32, y: i32) -> Point {
        let cell = |value: i32| u16::try_from(value).unwrap_or(u16::MAX);
        Point::new(cell(x - self.origin.0), cell(y - self.origin.1))
    }

    fn rect(&self, area: Area) -> Rect {
        let at = self.point(area.left, area.top);
        let size = |value: i32| u16::try_from(value).unwrap_or(0);
        Rect::new(at.x, at.y, size(area.right - area.left + 1), size(area.bottom - area.top + 1))
    }

    fn text(&mut self, x: i32, y: i32, text: &str, role: Role) {
        let at = self.point(x, y);
        self.canvas.text(at, text, role);
    }

    fn line(&mut self, from: (i32, i32), to: (i32, i32), kind: LineKind, role: Role) {
        let (from, to) = (self.point(from.0, from.1), self.point(to.0, to.1));
        self.canvas.line(from, to, kind, role).unwrap_or_default();
    }

    /// A frame with its title in the top border after `┌─ `, cut with an ellipsis to leave ` ─┐`.
    fn frame(&mut self, area: Area, kind: LineKind, corner: Corner, title: Option<&str>) {
        let rect = self.rect(area);
        self.canvas.frame(rect, Frame { kind, corner, title: None, role: Role::Group });
        if let Some(title) = title {
            self.title(area, area.top, title);
        }
    }

    /// A block's frame, square, dashed for a `rect`, and its dividers, dashed lines with their titles.
    fn block(&mut self, block: &Block) {
        let kind = if block.dashed { LineKind::Dashed } else { LineKind::Light };
        self.frame(block.area, kind, Corner::Square, Some(block.title.as_str()));
        for divider in &block.dividers {
            self.line((block.area.left, divider.row), (block.area.right, divider.row), LineKind::Dashed, Role::Group);
            self.title(block.area, divider.row, &divider.title);
        }
    }

    /// `title` on row `y` of a frame, between ` ` after its second column and ` ` before its last two, cut to fit.
    fn title(&mut self, area: Area, y: i32, title: &str) {
        let room = area.right - area.left - 5;
        if title.is_empty() || room <= 0 {
            return;
        }
        let written = i32::try_from(vt_canvas::width(title)).unwrap_or(i32::MAX).min(room);
        let at = self.point(area.left + 3, y);
        self.canvas.text_cut(at, title, u16::try_from(room).unwrap_or(0), Role::Group);
        self.text(area.left + 2, y, " ", Role::Group);
        self.text(area.left + 3 + written, y, " ", Role::Group);
    }

    /// Each line of `label` centred in `width` columns from `left`, cut with an ellipsis to the label's width.
    fn centred(&mut self, label: &Label, left: i32, top: i32, width: i32, role: Role) {
        for (row, line) in (top..).zip(&label.lines) {
            let line_width = i32::try_from(vt_canvas::width(line)).unwrap_or(i32::MAX).min(i32::from(label.width));
            let at = self.point(left + (width - line_width).max(0) / 2, row);
            self.canvas.text_cut(at, line, label.width, role);
        }
    }

    /// Each line of `label` from `at`, cut with an ellipsis to the label's width.
    fn left_aligned(&mut self, label: &Label, at: Cell, role: Role) {
        for (row, line) in (at.y..).zip(&label.lines) {
            let start = self.point(at.x, row);
            self.canvas.text_cut(start, line, label.width, role);
        }
    }

    /// Light, heavy where the participant is active; an activation of one row still shows on two.
    fn lifeline(&mut self, lifeline: &Lifeline) {
        if lifeline.bottom <= lifeline.top {
            return;
        }
        self.line((lifeline.x, lifeline.top), (lifeline.x, lifeline.bottom), LineKind::Light, Role::Edge);
        for span in lifeline.active.iter().filter(|span| span.top < lifeline.bottom) {
            let (start, end) = (span.top.max(lifeline.top), span.bottom.max(span.top + 1).min(lifeline.bottom));
            self.line((lifeline.x, start), (lifeline.x, end), LineKind::Heavy, Role::Edge);
        }
    }

    fn head(&mut self, head: &Head) {
        let area = head.area;
        let width = area.right - area.left + 1;
        match head.figure {
            Figure::Box => {
                let rect = self.rect(area);
                self.canvas.frame(rect, Frame { kind: LineKind::Light, corner: Corner::Square, title: None, role: Role::Node });
                self.centred(&head.label, area.left + 2, area.top + 1, width - 4, Role::Label);
            }
            Figure::Database => {
                let rect = self.rect(area);
                self.canvas.shape(rect, Shape::Cylinder, Role::Node);
                self.centred(&head.label, area.left + 2, area.top + 2, width - 4, Role::Label);
            }
            Figure::Person => {
                let middle = area.left + (width - 1) / 2;
                for (row, line) in (area.top..).zip(PERSON) {
                    self.text(middle - 1, row, line, Role::Node);
                }
                self.centred(&head.label, area.left, area.top + 3, width, Role::Label);
            }
        }
    }

    fn message(&mut self, message: &Message) {
        let kind = if message.dotted { LineKind::Dashed } else { LineKind::Light };
        let (start, end, row) = (message.start, message.end, message.row);
        if message.to_itself {
            let points = [self.point(start, row), self.point(end, row), self.point(end, row + 1), self.point(start, row + 1)];
            self.canvas.path(&points, kind, Role::Edge).unwrap_or_default();
            self.tip(message.tip, (start, row + 1), Direction::Left);
            self.tip(message.back_tip, (start, row), Direction::Left);
            self.left_aligned(&message.label, message.label_at, Role::Label);
            return;
        }
        let (forward, back) = if end >= start { (Direction::Right, Direction::Left) } else { (Direction::Left, Direction::Right) };
        self.line((start, row), (end, row), kind, Role::Edge);
        self.tip(message.tip, (end, row), forward);
        self.tip(message.back_tip, (start, row), back);
        self.centred(&message.label, message.label_at.x, message.label_at.y, i32::from(message.label.width), Role::Label);
    }

    fn tip(&mut self, tip: Tip, (x, y): (i32, i32), direction: Direction) {
        let at = self.point(x, y);
        match tip {
            Tip::None => {}
            Tip::Arrow => self.canvas.arrow(at, direction, Role::Edge),
            Tip::Cross => self.canvas.cross(at, Role::Edge),
        }
    }
}

/// The smallest area holding every part.
fn bounds(layout: &Layout) -> Area {
    let heads = layout.heads.iter().map(|head| head.area);
    let notes = layout.notes.iter().map(|note| note.area);
    let frames = layout.frames.iter().map(|frame| frame.area);
    let groups = layout.groups.iter().map(|group| group.area);
    let lifelines =
        layout.lifelines.iter().map(|lifeline| Area { left: lifeline.x, top: lifeline.top, right: lifeline.x, bottom: lifeline.bottom });
    let messages = layout.messages.iter().map(|message| {
        let label_right = message.label_at.x + i32::from(message.label.width) - 1;
        let row = message.row + i32::from(message.to_itself);
        Area {
            left: message.start.min(message.end).min(message.label_at.x),
            top: message.label_at.y.min(message.row),
            right: message.start.max(message.end).max(label_right),
            bottom: row,
        }
    });
    let all = heads.chain(notes).chain(frames).chain(groups).chain(lifelines).chain(messages);
    all.reduce(Area::union).unwrap_or(Area { left: 0, top: 0, right: 0, bottom: 0 })
}

/// The cells of the lifelines a message, a frame's border, a divider or a note passes in front of.
fn crossings(layout: &Layout) -> Vec<(i32, i32)> {
    let mut rows: Vec<(i32, i32, i32)> = Vec::new();
    for message in &layout.messages {
        let (left, right) = (message.start.min(message.end), message.start.max(message.end));
        rows.push((left, right, message.row));
        if message.to_itself {
            rows.push((left, right, message.row + 1));
        }
    }
    for frame in &layout.frames {
        let area = frame.area;
        let borders = [area.top, area.bottom].into_iter().chain(frame.dividers.iter().map(|divider| divider.row));
        rows.extend(borders.map(|y| (area.left + 1, area.right - 1, y)));
    }
    for note in &layout.notes {
        rows.extend((note.area.top..=note.area.bottom).map(|y| (note.area.left, note.area.right, y)));
    }
    for group in &layout.groups {
        rows.extend([group.area.top, group.area.bottom].map(|y| (group.area.left + 1, group.area.right - 1, y)));
    }
    layout
        .lifelines
        .iter()
        .flat_map(|lifeline| {
            rows.iter()
                .filter(move |&&(left, right, y)| (left..=right).contains(&lifeline.x) && (lifeline.top..=lifeline.bottom).contains(&y))
                .map(move |&(_, _, y)| (lifeline.x, y))
        })
        .collect()
}
