//! Where everything goes, in cells: each participant's column from the room the texts beside it need, then each event
//! on rows of its own, top to bottom.

mod columns;

use super::diagram::{Diagram, Event, Figure};
use super::parse::{Block, Placement as NotePlacement, Tip};
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
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Placement {
    /// Participants' heads, at the top or where they are created, then their feet at the bottom.
    pub(super) heads: Vec<Head>,
    pub(super) lifelines: Vec<Lifeline>,
    pub(super) messages: Vec<Message>,
    pub(super) notes: Vec<Note>,
    pub(super) frames: Vec<Frame>,
    pub(super) groups: Vec<Titled>,
    /// Where a destroyed participant's lifeline ends.
    pub(super) crosses: Vec<(i32, i32)>,
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
    pub(super) active: Vec<(i32, i32)>,
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
    pub(super) label_at: (i32, i32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Note {
    pub(super) area: Area,
    pub(super) label: Label,
}

/// A block's frame, its title and its dividers' rows and titles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Frame {
    pub(super) area: Area,
    pub(super) title: String,
    pub(super) dashed: bool,
    pub(super) dividers: Vec<(i32, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Titled {
    pub(super) area: Area,
    pub(super) title: Option<String>,
}

/// Lays `diagram` out as tight as `fit` asks.
pub(super) fn place(diagram: &Diagram, fit: Fit) -> Placement {
    let shapes: Vec<Head> =
        diagram.participants.iter().map(|participant| head(participant.figure, Label::wrapped_as(&participant.label, fit.wrap))).collect();
    let labels: Vec<Label> = diagram.events.iter().map(|event| event_label(event, fit.wrap)).collect();
    let columns = columns::columns(diagram, &shapes, &labels, fit.gap);
    Rows::new(diagram, &shapes, &columns, fit.wrap.columns).placed(&labels)
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
    shapes: &'a [Head],
    columns: &'a [i32],
    /// The widest a frame's title widens it; a longer one is cut.
    title_room: usize,
    placement: Placement,
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
    dividers: Vec<(i32, String)>,
}

impl<'a> Rows<'a> {
    fn new(diagram: &'a Diagram, shapes: &'a [Head], columns: &'a [i32], title_room: usize) -> Self {
        let top = i32::from(!diagram.groups.is_empty());
        let placement = Placement {
            heads: Vec::new(),
            lifelines: Vec::new(),
            messages: Vec::new(),
            notes: Vec::new(),
            frames: Vec::new(),
            groups: Vec::new(),
            crosses: Vec::new(),
        };
        let mut rows = Self {
            diagram,
            shapes,
            columns,
            title_room,
            placement,
            mark: top,
            cursor: top,
            activations: vec![Vec::new(); shapes.len()],
            ended: vec![false; shapes.len()],
            open: Vec::new(),
            extents: vec![None; diagram.events.len()],
        };
        rows.heads(top);
        rows
    }

    /// The heads of the participants there from the start, and the top of every lifeline.
    fn heads(&mut self, top: i32) {
        let present = self.shapes.iter().zip(&self.diagram.participants).filter(|(_, participant)| participant.created.is_none());
        let tallest = present.map(|(shape, _)| shape.height()).max().unwrap_or(0);
        for (place, shape) in self.shapes.iter().enumerate() {
            let x = self.columns[place];
            let created = self.diagram.participants[place].created.is_some();
            let head = shape.at(x, top);
            self.placement.lifelines.push(Lifeline { x, top: below(&head), bottom: 0, active: Vec::new() });
            if !created {
                self.placement.heads.push(head);
            }
        }
        self.bands(top, tallest, true);
        self.cursor = top + tallest + i32::from(!self.diagram.groups.is_empty());
        self.mark = self.cursor - 1;
    }

    /// A frame around each box's participants, heads or feet, from the row above them to the row under them; the top
    /// one holds the box's name.
    fn bands(&mut self, top: i32, tallest: i32, named: bool) {
        for (index, group) in self.diagram.groups.iter().enumerate() {
            let members = self.diagram.participants.iter().enumerate().filter(|(_, participant)| participant.group == Some(index));
            let spans = members
                .map(|(place, _)| (self.columns[place] - self.shapes[place].before(), self.columns[place] + self.shapes[place].after()));
            let Some((left, right)) = spans.reduce(|one, other| (one.0.min(other.0), one.1.max(other.1))) else { continue };
            let area = Area { left: left - GROUP_PADDING, top: top - 1, right: right + GROUP_PADDING, bottom: top + tallest };
            self.placement.groups.push(Titled { area, title: group.name.clone().filter(|_| named) });
        }
    }

    fn placed(mut self, labels: &[Label]) -> Placement {
        for (index, (event, label)) in self.diagram.events.iter().zip(labels).enumerate() {
            match *event {
                Event::Message { from, to, arrow, .. } => {
                    self.cursor += 1;
                    let message = if from == to { self.looped(from, label) } else { self.between(index, from, to, label) };
                    let destroys_receiver = from != to && self.diagram.participants[to].destroyed == Some(index);
                    let tip = if destroys_receiver && arrow.tip() == Tip::Cross { Tip::None } else { arrow.tip() };
                    let message = Message { dotted: arrow.is_dotted(), tip, back_tip: arrow.back_tip(), ..message };
                    self.extents[index] = Some(extent(&message));
                    self.destroy(index, [from, to], message.row);
                    self.mark = message.row + i32::from(message.to_itself);
                    self.placement.messages.push(message);
                }
                Event::Note { first, last, placement, .. } => {
                    self.cursor += 1;
                    let area = self.note(first, last, placement, label);
                    self.extents[index] = Some((area.left, area.right));
                    self.cursor = area.bottom + 1;
                    self.mark = area.bottom;
                    self.placement.notes.push(Note { area, label: label.clone() });
                }
                Event::Activation { participant, start: true } => self.activations[participant].push(self.mark),
                Event::Activation { participant, start: false } => {
                    if let Some(start) = self.activations[participant].pop() {
                        self.placement.lifelines[participant].active.push((start, self.mark));
                    }
                }
                Event::Open { block, ref title } => {
                    self.cursor += 1;
                    self.open.push(OpenFrame {
                        top: self.cursor,
                        event: index,
                        title: title.clone(),
                        dashed: block == Block::Rect,
                        dividers: Vec::new(),
                    });
                    self.cursor += 1;
                }
                Event::Section { ref title } => {
                    self.cursor += 1;
                    if let Some(frame) = self.open.last_mut() {
                        frame.dividers.push((self.cursor, title.clone()));
                    }
                    self.cursor += 1;
                }
                Event::Close => {
                    self.cursor += 1;
                    self.close(index);
                    self.cursor += 1;
                }
            }
        }
        self.feet()
    }

    /// A message between two participants: its label over the line, centred between the lifelines, or between the
    /// sender and the head of the participant it creates.
    fn between(&mut self, index: usize, from: usize, to: usize, label: &Label) -> Message {
        let direction = if to > from { 1 } else { -1 };
        let label_rows = label_rows(label);
        let label_top = self.cursor;
        let created = self.diagram.participants[to].created == Some(index);
        let (row, target) = if created {
            let head = self.shapes[to].at(self.columns[to], label_top + label_rows);
            let row = head.area.top + head.height() / 2;
            let edge = if direction > 0 { head.area.left } else { head.area.right };
            self.cursor = head.area.bottom + 1;
            self.placement.lifelines[to].top = below(&head);
            self.placement.heads.push(head);
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
            label_at: (label_x, label_top),
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
            label_at: (x + 2, row - label_rows(label)),
        }
    }

    fn note(&self, first: usize, last: usize, placement: NotePlacement, label: &Label) -> Area {
        let width = i32::from(label.width) + 4;
        let height = i32::from(label.height()) + 2;
        let (low, high) = (self.columns[first], self.columns[last]);
        let left = match placement {
            NotePlacement::RightOf => high + 2,
            NotePlacement::LeftOf => low - 1 - width,
            NotePlacement::Over => {
                let width = width.max(high - low + 5);
                return Area::sized(i32::midpoint(low, high) - (width - 1) / 2, self.cursor, width, height);
            }
        };
        Area::sized(left, self.cursor, width, height)
    }

    /// The lifelines of the participants a message destroys end on its row, with a cross.
    fn destroy(&mut self, index: usize, involved: [usize; 2], row: i32) {
        for place in involved {
            if self.diagram.participants[place].destroyed == Some(index) && !self.ended[place] {
                self.ended[place] = true;
                self.placement.lifelines[place].bottom = row;
                self.placement.crosses.push((self.columns[place], row));
            }
        }
    }

    /// A block's frame around what it holds, nested frames included, and wide enough for its title cut to the width
    /// labels wrap to.
    fn close(&mut self, index: usize) {
        let Some(open) = self.open.pop() else { return };
        let inside = open.event + 1..index;
        let held = self.extents[inside].iter().flatten().copied().reduce(|one, other| (one.0.min(other.0), one.1.max(other.1)));
        let first = self.columns.first().copied().unwrap_or(0);
        let last = self.columns.last().copied().unwrap_or(0);
        let (left, right) = held.unwrap_or((first, last));
        let (left, right) = (left - FRAME_PADDING, right + FRAME_PADDING);
        let title_width = i32::try_from(vt_canvas::width(&open.title).min(self.title_room)).unwrap_or(i32::MAX);
        let right = right.max(left + title_width + 5);
        let area = Area { left, top: open.top, right, bottom: self.cursor };
        self.extents[open.event] = Some((left, right));
        self.extents[index] = Some((left, right));
        self.placement.frames.push(Frame { area, title: open.title, dashed: open.dashed, dividers: open.dividers });
    }

    /// The feet of the participants still there at the end, the lifelines down to them, and the boxes around them.
    fn feet(mut self) -> Placement {
        let bottom = self.cursor;
        for (place, active) in self.activations.iter().enumerate() {
            let spans = active.iter().map(|&start| (start, bottom));
            self.placement.lifelines[place].active.extend(spans);
        }
        let top = self.cursor + 1 + i32::from(!self.diagram.groups.is_empty());
        let mut tallest = 0;
        for (place, shape) in self.shapes.iter().enumerate() {
            if self.ended[place] {
                continue;
            }
            let foot = shape.at(self.columns[place], top);
            self.placement.lifelines[place].bottom = if foot.figure == Figure::Person { top - 1 } else { top };
            self.placement.heads.push(foot);
            tallest = tallest.max(shape.height());
        }
        self.bands(top, tallest, false);
        self.placement
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
    let label_end = message.label_at.0 + i32::from(message.label.width) - 1;
    (low.min(message.label_at.0), high.max(label_end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::LongWords;

    fn placed(body: &str) -> Placement {
        place(&Diagram::read(&format!("sequenceDiagram\n{body}")).unwrap(), Fit { wrap: Wrap::wide(LongWords::Break), gap: LOOSE })
    }

    #[test]
    fn a_frame_keeps_its_padding_from_the_participants_outside_it() {
        let placement = placed("participant A\nparticipant B\nparticipant C\nloop\nloop\nB->>C: hi\nend\nend\n");

        let outer = placement.frames.last().unwrap().area;
        assert!(outer.left > placement.lifelines[0].x, "{outer:?}");
    }

    #[test]
    fn an_activation_runs_from_the_message_that_starts_it_to_the_one_that_ends_it() {
        let placement = placed("A->>+B: go\nB-->>-A: done\n");

        let rows: Vec<i32> = placement.messages.iter().map(|message| message.row).collect();
        assert_eq!(placement.lifelines[1].active, [(rows[0], rows[1])]);
    }

    #[test]
    fn a_created_participant_has_its_head_where_the_message_creating_it_arrives() {
        let placement = placed("A->>B: hi\ncreate participant C\nB->>C: new\n");

        let head = placement.heads.iter().find(|head| head.label.lines == ["C"]).unwrap();
        let message = &placement.messages[1];
        assert_eq!((message.row, message.end + 1), (head.area.top + 1, head.area.left));
    }

    #[test]
    fn a_destroyed_participant_has_no_foot_and_a_cross_where_its_lifeline_ends() {
        let placement = placed("A->>B: hi\ndestroy B\nA->>B: bye\n");

        assert_eq!(placement.crosses, [(placement.lifelines[1].x, placement.messages[1].row)]);
        assert_eq!(placement.heads.len(), 3);
    }

    #[test]
    fn a_crossed_message_destroying_its_receiver_ends_on_one_cross() {
        let placement = placed("A->>B: hi\ndestroy B\nA-xB: bye\n");

        assert_eq!(placement.messages[1].tip, Tip::None);
        assert_eq!(placement.crosses, [(placement.lifelines[1].x, placement.messages[1].row)]);
    }

    #[test]
    fn participants_created_later_leave_no_room_among_the_heads() {
        let placement = placed("A->>B: hi\ncreate actor C\nB->>C: new\n");

        assert_eq!(placement.messages[0].label_at.1, 4);
    }
}
