//! Where everything goes, in cells: each participant's column from the room the texts beside it need, then each event
//! on rows of its own, top to bottom.

mod columns;

use super::diagram::{Diagram, Event, Figure};
use super::parse::{Arrow, Block, Placement, Tip};
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

/// Where each part goes, in cells that may start left of 0: the drawing moves them.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Layout {
    /// Participants' heads, at the top or where they are created, then their feet at the bottom.
    pub(super) heads: Vec<Head>,
    pub(super) lifelines: Vec<Lifeline>,
    pub(super) messages: Vec<Message>,
    pub(super) notes: Vec<Note>,
    pub(super) frames: Vec<Frame>,
    pub(super) groups: Vec<Titled>,
    /// Where a destroyed participant's lifeline ends.
    pub(super) crosses: Vec<Cell>,
}

/// A cell, where the layout's columns and rows start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Cell {
    pub(super) x: i32,
    pub(super) y: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Head {
    pub(super) area: Area,
    pub(super) figure: Figure,
    pub(super) label: Label,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Lifeline {
    pub(super) x: i32,
    pub(super) top: i32,
    pub(super) bottom: i32,
    /// Rows where the participant is active, drawn heavy.
    pub(super) active: Vec<Span>,
}

/// Rows from `top` to `bottom`, both included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Span {
    pub(super) top: i32,
    pub(super) bottom: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Message {
    pub(super) row: i32,
    /// The first cell of the line, next to the sender, and its last, next to the receiver.
    pub(super) start: i32,
    pub(super) end: i32,
    /// A message to its sender goes right from `start` to `end`, down a row and back.
    pub(super) to_itself: bool,
    pub(super) dotted: bool,
    pub(super) tip: Tip,
    pub(super) back_tip: Tip,
    pub(super) label: Label,
    /// Where the label's first line starts.
    pub(super) label_at: Cell,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Note {
    pub(super) area: Area,
    pub(super) label: Label,
}

/// A block's frame, its title and its dividers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Frame {
    pub(super) area: Area,
    pub(super) title: String,
    pub(super) dashed: bool,
    pub(super) dividers: Vec<Divider>,
}

/// `else`, `and` or `option`: a dashed line across a block, with its title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Divider {
    pub(super) row: i32,
    pub(super) title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Titled {
    pub(super) area: Area,
    pub(super) title: Option<String>,
}

/// Lays `diagram` out as tight as `fit` asks.
pub(super) fn place(diagram: &Diagram, fit: Fit) -> Layout {
    let heads: Vec<Head> =
        diagram.participants.iter().map(|participant| head(participant.figure, Label::wrapped_as(&participant.label, fit.wrap))).collect();
    let labels: Vec<Label> = diagram.events.iter().map(|event| event_label(event, fit.wrap)).collect();
    let columns = columns::columns(diagram, &heads, &labels, fit.gap);
    Rows::new(diagram, &heads, &columns, fit.wrap.columns).placed(&labels)
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

    fn at(&self, x: i32, top: i32) -> Self {
        Self { area: Area::sized(x - self.before(), top, self.width(), self.height()), ..self.clone() }
    }
}

/// The rows, laid out event after event, one empty row between two.
#[derive(Debug)]
struct Rows<'a> {
    diagram: &'a Diagram,
    /// Each participant's head, at the origin.
    heads: &'a [Head],
    columns: &'a [i32],
    /// The widest a frame's or a divider's title widens a block; a longer one is cut.
    title_room: usize,
    layout: Layout,
    /// The row an activation starts or ends on: the last message's arrow, or a note's last row.
    mark: i32,
    cursor: i32,
    activations: Vec<Vec<i32>>,
    /// Whether each participant's lifeline has ended with its destruction.
    ended: Vec<bool>,
    open: Vec<OpenFrame>,
    /// Each event's columns, for the frames around it.
    extents: Vec<Option<(i32, i32)>>,
}

/// A block whose end is not read yet.
#[derive(Debug)]
struct OpenFrame {
    top: i32,
    event: usize,
    title: String,
    dashed: bool,
    dividers: Vec<Divider>,
}

impl<'a> Rows<'a> {
    fn new(diagram: &'a Diagram, heads: &'a [Head], columns: &'a [i32], title_room: usize) -> Self {
        let top = i32::from(!diagram.groups.is_empty());
        let mut rows = Self {
            diagram,
            heads,
            columns,
            title_room,
            layout: Layout::default(),
            mark: top,
            cursor: top,
            activations: vec![Vec::new(); heads.len()],
            ended: vec![false; heads.len()],
            open: Vec::new(),
            extents: vec![None; diagram.events.len()],
        };
        rows.top_heads(top);
        rows
    }

    /// The heads of the participants there from the start, their boxes, and the top of every lifeline.
    fn top_heads(&mut self, top: i32) {
        let present = self.heads.iter().zip(&self.diagram.participants).filter(|(_, participant)| participant.created.is_none());
        let tallest = present.map(|(head, _)| head.height()).max().unwrap_or(0);
        for (place, head) in self.heads.iter().enumerate() {
            let x = self.columns[place];
            let head = head.at(x, top);
            self.layout.lifelines.push(Lifeline { x, top: below(&head), bottom: 0, active: Vec::new() });
            if self.diagram.participants[place].created.is_none() {
                self.layout.heads.push(head);
            }
        }
        let names = self.diagram.groups.iter().map(|group| group.name.clone());
        let bands = self.bands(top, tallest).into_iter().zip(names).map(|(area, title)| Titled { area, title });
        self.layout.groups.extend(bands);
        self.cursor = top + tallest + i32::from(!self.diagram.groups.is_empty());
        self.mark = self.cursor - 1;
    }

    /// The area around each box's participants, heads or feet, from the row above them to the row under them.
    fn bands(&self, top: i32, tallest: i32) -> Vec<Area> {
        let column_span =
            |place: usize| (self.columns[place] - self.heads[place].before(), self.columns[place] + self.heads[place].after());
        (0..self.diagram.groups.len())
            .filter_map(|group| {
                let members = (0..self.heads.len()).filter(|&place| self.diagram.participants[place].group == Some(group));
                let (left, right) = members.map(column_span).reduce(|one, other| (one.0.min(other.0), one.1.max(other.1)))?;
                Some(Area { left: left - GROUP_PADDING, top: top - 1, right: right + GROUP_PADDING, bottom: top + tallest })
            })
            .collect()
    }

    fn placed(mut self, labels: &[Label]) -> Layout {
        for (index, (event, label)) in self.diagram.events.iter().zip(labels).enumerate() {
            match *event {
                Event::Message { from, to, arrow, .. } => self.message(index, from, to, arrow, label),
                Event::Note { first, last, placement, .. } => self.note(index, first, last, placement, label),
                Event::Activation { participant, start } => self.activate(participant, start),
                Event::Open { block, ref title } => self.open(index, block, title),
                Event::Section { ref title } => self.section(title),
                Event::Close => self.close(index),
            }
        }
        self.feet()
    }

    fn message(&mut self, index: usize, from: usize, to: usize, arrow: Arrow, label: &Label) {
        self.cursor += 1;
        let placed = if from == to { self.looped(from, label) } else { self.between(index, from, to, label) };
        let message = Message { dotted: arrow.is_dotted(), tip: self.tip(index, from, to, arrow), back_tip: arrow.back_tip(), ..placed };
        self.extents[index] = Some(extent(&message));
        self.destroy(index, [from, to], message.row);
        self.mark = message.row + i32::from(message.to_itself);
        self.layout.messages.push(message);
    }

    /// The arrow's tip, but none for a cross on a receiver the message destroys, as its lifeline ends on a cross.
    fn tip(&self, index: usize, from: usize, to: usize, arrow: Arrow) -> Tip {
        let destroys_receiver = from != to && self.diagram.participants[to].destroyed == Some(index);
        if destroys_receiver && arrow.tip() == Tip::Cross { Tip::None } else { arrow.tip() }
    }

    /// A message between two participants: its label over the line, centred between the lifelines, or between the
    /// sender and the head of the participant it creates.
    fn between(&mut self, index: usize, from: usize, to: usize, label: &Label) -> Message {
        let direction = if to > from { 1 } else { -1 };
        let label_rows = label_rows(label);
        let label_top = self.cursor;
        let created = self.diagram.participants[to].created == Some(index);
        let (row, target) = if created {
            let head = self.heads[to].at(self.columns[to], label_top + label_rows);
            let row = head.area.top + head.height() / 2;
            let edge = if direction > 0 { head.area.left } else { head.area.right };
            self.cursor = head.area.bottom + 1;
            self.layout.lifelines[to].top = below(&head);
            self.layout.heads.push(head);
            (row, edge)
        } else {
            self.cursor = label_top + label_rows + 1;
            (label_top + label_rows, self.columns[to])
        };
        let source = self.columns[from];
        let neighbour = if to > from { from + 1 } else { from - 1 };
        let next = if neighbour == to { target } else { self.columns[neighbour] };
        let label_x = label_left(label, source, next).unwrap_or_else(|| label_left_anyway(label, source, target));
        Message {
            row,
            start: source + direction,
            end: target - direction,
            to_itself: false,
            dotted: false,
            tip: Tip::None,
            back_tip: Tip::None,
            label: label.clone(),
            label_at: Cell { x: label_x, y: label_top },
        }
    }

    /// A message to its sender: its label above a loop going right of the lifeline and back.
    fn looped(&mut self, place: usize, label: &Label) -> Message {
        let x = self.columns[place];
        let row = self.cursor + label_rows(label);
        self.cursor = row + 2;
        Message {
            row,
            start: x + 1,
            end: x + 3,
            to_itself: true,
            dotted: false,
            tip: Tip::None,
            back_tip: Tip::None,
            label: label.clone(),
            label_at: Cell { x: x + 2, y: row - label_rows(label) },
        }
    }

    fn note(&mut self, index: usize, first: usize, last: usize, placement: Placement, label: &Label) {
        self.cursor += 1;
        let area = self.note_area(first, last, placement, label);
        self.extents[index] = Some((area.left, area.right));
        self.cursor = area.bottom + 1;
        self.mark = area.bottom;
        self.layout.notes.push(Note { area, label: label.clone() });
    }

    fn note_area(&self, first: usize, last: usize, placement: Placement, label: &Label) -> Area {
        let width = i32::from(label.width) + 4;
        let height = i32::from(label.height()) + 2;
        let (low, high) = (self.columns[first], self.columns[last]);
        let left = match placement {
            Placement::RightOf => high + 2,
            Placement::LeftOf => low - 1 - width,
            Placement::Over => {
                let width = width.max(high - low + 5);
                return Area::sized(i32::midpoint(low, high) - (width - 1) / 2, self.cursor, width, height);
            }
        };
        Area::sized(left, self.cursor, width, height)
    }

    /// An activation starts on the row of what came last, a deactivation ends the latest one there.
    fn activate(&mut self, participant: usize, start: bool) {
        if start {
            self.activations[participant].push(self.mark);
        } else if let Some(top) = self.activations[participant].pop() {
            self.layout.lifelines[participant].active.push(Span { top, bottom: self.mark });
        }
    }

    fn open(&mut self, index: usize, block: Block, title: &str) {
        self.cursor += 1;
        let frame =
            OpenFrame { top: self.cursor, event: index, title: title.to_owned(), dashed: block == Block::Rect, dividers: Vec::new() };
        self.open.push(frame);
        self.cursor += 1;
    }

    fn section(&mut self, title: &str) {
        self.cursor += 1;
        let divider = Divider { row: self.cursor, title: title.to_owned() };
        if let Some(frame) = self.open.last_mut() {
            frame.dividers.push(divider);
        }
        self.cursor += 1;
    }

    /// The lifelines of the participants a message destroys end on its row, with a cross.
    fn destroy(&mut self, index: usize, involved: [usize; 2], row: i32) {
        for place in involved {
            if self.diagram.participants[place].destroyed == Some(index) && !self.ended[place] {
                self.ended[place] = true;
                self.layout.lifelines[place].bottom = row;
                self.layout.crosses.push(Cell { x: self.columns[place], y: row });
            }
        }
    }

    /// A block's frame around what it holds, nested frames included, and wide enough for its title and its dividers'
    /// cut to the width labels wrap to.
    fn close(&mut self, index: usize) {
        self.cursor += 1;
        let Some(open) = self.open.pop() else { return };
        let inside = open.event + 1..index;
        let held = self.extents[inside].iter().flatten().copied().reduce(|one, other| (one.0.min(other.0), one.1.max(other.1)));
        let first = self.columns.first().copied().unwrap_or(0);
        let last = self.columns.last().copied().unwrap_or(0);
        let (left, right) = held.unwrap_or((first, last));
        let (left, right) = (left - FRAME_PADDING, right + FRAME_PADDING);
        let titles = std::iter::once(&open.title).chain(open.dividers.iter().map(|divider| &divider.title));
        let title_width = titles.map(|title| vt_canvas::text_width(title).min(self.title_room)).max().unwrap_or(0);
        let right = right.max(left + i32::try_from(title_width).unwrap_or(i32::MAX) + 5);
        let area = Area { left, top: open.top, right, bottom: self.cursor };
        self.extents[open.event] = Some((left, right));
        self.extents[index] = Some((left, right));
        self.layout.frames.push(Frame { area, title: open.title, dashed: open.dashed, dividers: open.dividers });
        self.cursor += 1;
    }

    /// The feet of the participants still there at the end, the lifelines down to them, and the boxes around them.
    fn feet(mut self) -> Layout {
        let bottom = self.cursor;
        for (place, active) in self.activations.iter().enumerate() {
            let spans = active.iter().map(|&top| Span { top, bottom });
            self.layout.lifelines[place].active.extend(spans);
        }
        let top = self.cursor + 1 + i32::from(!self.diagram.groups.is_empty());
        let mut tallest = 0;
        for (place, head) in self.heads.iter().enumerate() {
            if self.ended[place] {
                continue;
            }
            let foot = head.at(self.columns[place], top);
            self.layout.lifelines[place].bottom = if foot.figure == Figure::Person { top - 1 } else { top };
            self.layout.heads.push(foot);
            tallest = tallest.max(head.height());
        }
        let bands = self.bands(top, tallest).into_iter().map(|area| Titled { area, title: None });
        self.layout.groups.extend(bands);
        self.layout
    }
}

/// Where a label goes centred between two columns, when it fits there with a space each side.
fn label_left(label: &Label, one: i32, other: i32) -> Option<i32> {
    let (low, high) = (one.min(other), one.max(other));
    let room = high - low - 1;
    (room >= i32::from(label.width) + 2).then(|| low + 1 + (room - i32::from(label.width)) / 2)
}

/// Centred between two columns, over the lifelines between them if it must.
fn label_left_anyway(label: &Label, one: i32, other: i32) -> i32 {
    let (low, high) = (one.min(other), one.max(other));
    low + 1 + (high - low - 1 - i32::from(label.width)) / 2
}

/// The first row of a lifeline: a head's bottom border, which it joins, or the row under a stick figure's name.
fn below(head: &Head) -> i32 {
    if head.figure == Figure::Person { head.area.bottom + 1 } else { head.area.bottom }
}

fn label_rows(label: &Label) -> i32 {
    if label.lines.iter().all(String::is_empty) { 0 } else { i32::from(label.height()) }
}

/// The columns a message covers, its label included.
fn extent(message: &Message) -> (i32, i32) {
    let (low, high) = (message.start.min(message.end) - 1, message.start.max(message.end) + 1);
    let label_end = message.label_at.x + i32::from(message.label.width) - 1;
    (low.min(message.label_at.x), high.max(label_end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::LongWords;

    fn placed(body: &str) -> Layout {
        place(&Diagram::read_source(&format!("sequenceDiagram\n{body}")).unwrap(), Fit { wrap: Wrap::wide(LongWords::Break), gap: LOOSE })
    }

    #[test]
    fn a_frame_keeps_its_padding_from_the_participants_outside_it() {
        let layout = placed("participant A\nparticipant B\nparticipant C\nloop\nloop\nB->>C: hi\nend\nend\n");

        let outer = layout.frames.last().unwrap().area;
        assert!(outer.left > layout.lifelines[0].x, "{outer:?}");
    }

    #[test]
    fn an_activation_runs_from_the_message_that_starts_it_to_the_one_that_ends_it() {
        let layout = placed("A->>+B: go\nB-->>-A: done\n");

        let rows: Vec<i32> = layout.messages.iter().map(|message| message.row).collect();
        assert_eq!(layout.lifelines[1].active, [Span { top: rows[0], bottom: rows[1] }]);
    }

    #[test]
    fn a_created_participant_has_its_head_where_the_message_creating_it_arrives() {
        let layout = placed("A->>B: hi\ncreate participant C\nB->>C: new\n");

        let head = layout.heads.iter().find(|head| head.label.lines == ["C"]).unwrap();
        let message = &layout.messages[1];
        assert_eq!((message.row, message.end + 1), (head.area.top + 1, head.area.left));
    }

    #[test]
    fn a_destroyed_participant_has_no_foot_and_a_cross_where_its_lifeline_ends() {
        let layout = placed("A->>B: hi\ndestroy B\nA->>B: bye\n");

        assert_eq!(layout.crosses, [Cell { x: layout.lifelines[1].x, y: layout.messages[1].row }]);
        assert_eq!(layout.heads.len(), 3);
    }

    #[test]
    fn a_crossed_message_destroying_its_receiver_ends_on_one_cross() {
        let layout = placed("A->>B: hi\ndestroy B\nA-xB: bye\n");

        assert_eq!(layout.messages[1].tip, Tip::None);
        assert_eq!(layout.crosses, [Cell { x: layout.lifelines[1].x, y: layout.messages[1].row }]);
    }

    #[test]
    fn a_divider_title_widens_its_block_up_to_the_width_labels_wrap_to() {
        let layout = placed("A->>B: x\nalt ok\nA->>B: y\nelse a long else condition that goes on\nB->>A: z\nend\n");

        let frame = &layout.frames[0];
        assert_eq!(frame.area.right - frame.area.left, 30 + 5);
    }

    #[test]
    fn participants_created_later_leave_no_room_among_the_heads() {
        let layout = placed("A->>B: hi\ncreate actor C\nB->>C: new\n");

        assert_eq!(layout.messages[0].label_at.y, 4);
    }
}
