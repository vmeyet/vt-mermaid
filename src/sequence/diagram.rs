//! What a sequence diagram draws: its participants side by side, then what happens between them, top to bottom.

use super::parse::{self, Arrow, Block, Item, Limits, Placement, Sequence};
use crate::error::Error;
use crate::source::entity;

/// A sequence diagram as it is drawn: texts shown as a browser shows them, participants by their place.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Diagram {
    pub(super) participants: Vec<Participant>,
    pub(super) events: Vec<Event>,
    pub(super) groups: Vec<Group>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Participant {
    pub(super) label: String,
    pub(super) figure: Figure,
    pub(super) group: Option<usize>,
    /// The event of the message that creates it, drawn with its head there instead of at the top.
    pub(super) created: Option<usize>,
    /// The event of the message after which its lifeline ends.
    pub(super) destroyed: Option<usize>,
}

/// How a participant is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Figure {
    Box,
    /// A stick figure over the name: mermaid's `actor`.
    Person,
    /// A cylinder: mermaid's `database`.
    Database,
}

/// A box around participants, by the places of the first and the last it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Group {
    pub(super) name: Option<String>,
    pub(super) first: usize,
    pub(super) last: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Event {
    Message {
        from: usize,
        to: usize,
        text: String,
        arrow: Arrow,
    },
    Note {
        first: usize,
        last: usize,
        placement: Placement,
        text: String,
    },
    Activation {
        participant: usize,
        start: bool,
    },
    /// A block's top, with its title: `loop`, then its text.
    Open {
        block: Block,
        title: String,
    },
    /// `else`, `and` or `option` dividing the block open.
    Section {
        title: String,
    },
    Close,
}

impl Diagram {
    /// The diagram in `source`, read within the limits `render` draws.
    pub(super) fn read(source: &str) -> Result<Self, Error> {
        parse::parse(source, Limits::DRAWN).map(|sequence| Self::from(&sequence))
    }

    fn from(sequence: &Sequence) -> Self {
        let place = |id: &str| sequence.participants.iter().position(|participant| participant.id == id);
        let mut numbers = Numbers { next: 1.0, step: 1.0, visible: false };
        let mut events = Vec::new();
        let mut items = Vec::new();
        for (index, item) in sequence.items.iter().enumerate() {
            if let Some(event) = event(item, &place, &mut numbers) {
                events.push(event);
                items.push(index);
            }
        }
        let participants = sequence
            .participants
            .iter()
            .enumerate()
            .map(|(place, participant)| Participant {
                label: entity::decoded(&participant.description),
                figure: figure(&participant.figure),
                group: participant.group,
                created: participant.created.and_then(|item| first_message(&events, &items, item, place)),
                destroyed: participant.destroyed.and_then(|item| first_message(&events, &items, item, place)),
            })
            .collect();
        let groups = sequence
            .groups
            .iter()
            .enumerate()
            .filter_map(|(index, group)| {
                let places: Vec<usize> =
                    sequence.participants.iter().enumerate().filter(|(_, p)| p.group == Some(index)).map(|(at, _)| at).collect();
                Some(Group { name: group.name.as_deref().map(entity::decoded), first: *places.first()?, last: *places.last()? })
            })
            .collect();
        Self { participants, events, groups }
    }
}

/// Mermaid's message numbers: they count every message, shown or not, from 1 by 1 unless `autonumber` says otherwise.
#[derive(Debug, Clone, Copy)]
struct Numbers {
    next: f64,
    step: f64,
    visible: bool,
}

impl Numbers {
    /// The text of the next message, after its number when numbers are shown.
    fn numbered(&mut self, text: &str) -> String {
        let number = self.next;
        self.next = ((self.next + self.step) * 100.0).round() / 100.0;
        match (self.visible, text.is_empty()) {
            (false, _) => text.to_owned(),
            (true, true) => number.to_string(),
            (true, false) => format!("{number}. {text}"),
        }
    }
}

/// The event an item draws, if it draws one; an item naming a participant that is not there draws nothing.
fn event(item: &Item, place: &impl Fn(&str) -> Option<usize>, numbers: &mut Numbers) -> Option<Event> {
    match item {
        Item::Message { from, to, text, arrow, .. } => {
            let (from, to) = (place(from)?, place(to)?);
            Some(Event::Message { from, to, text: numbers.numbered(&entity::decoded(&text.value)), arrow: *arrow })
        }
        Item::Note { from, to, placement, text } => {
            let (one, other) = (place(from)?, place(to)?);
            Some(Event::Note { first: one.min(other), last: one.max(other), placement: *placement, text: entity::decoded(&text.value) })
        }
        Item::Activation { id, start } => Some(Event::Activation { participant: place(id)?, start: *start }),
        Item::Autonumber { start, step, visible } => {
            numbers.next = start.filter(|start| *start != 0.0).unwrap_or(numbers.next);
            numbers.step = step.filter(|step| *step != 0.0).unwrap_or(numbers.step);
            numbers.visible = *visible;
            None
        }
        Item::Open { block, text } => Some(Event::Open { block: *block, title: title(*block, &text.value) }),
        Item::Section { text, .. } => Some(Event::Section { title: bracketed(&text.value) }),
        Item::Close { .. } => Some(Event::Close),
        Item::Central { .. } => None,
    }
}

/// `loop [Every minute]`; a `rect`'s text is its colour, so it has no title.
fn title(block: Block, text: &str) -> String {
    let keyword = match block {
        Block::Loop => "loop",
        Block::Rect => return String::new(),
        Block::Opt => "opt",
        Block::Alt => "alt",
        Block::Par | Block::ParOver => "par",
        Block::Critical => "critical",
        Block::Break => "break",
    };
    let condition = bracketed(text);
    if condition.is_empty() { keyword.to_owned() } else { format!("{keyword} {condition}") }
}

fn bracketed(text: &str) -> String {
    let text = entity::decoded(text);
    if text.is_empty() { text } else { format!("[{text}]") }
}

fn figure(name: &str) -> Figure {
    match name {
        "actor" => Figure::Person,
        "database" => Figure::Database,
        _ => Figure::Box,
    }
}

/// The first message from `item` on that involves the participant at `place`: mermaid records a creation or a
/// destruction by the item after it, and checks only that the next message involves the participant.
fn first_message(events: &[Event], items: &[usize], item: usize, place: usize) -> Option<usize> {
    let first = items.partition_point(|&index| index < item);
    (first..events.len()).find(|&event| matches!(events[event], Event::Message { from, to, .. } if from == place || to == place))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(diagram: &Diagram) -> Vec<String> {
        diagram
            .events
            .iter()
            .filter_map(|event| match event {
                Event::Message { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn numbers_count_every_message_and_show_after_autonumber() {
        let diagram =
            Diagram::read("sequenceDiagram\nA->>B: one\nautonumber\nA->>B: two\nautonumber 10 5\nA->>B: three\nB->>A: \n").unwrap();

        assert_eq!(texts(&diagram), ["one", "2. two", "10. three", "15"]);
    }

    #[test]
    fn numbers_keep_two_decimals_at_most() {
        let diagram = Diagram::read("sequenceDiagram\nautonumber 1 0.1\nA->>B: a\nA->>B: b\nA->>B: c\nA->>B: d\n").unwrap();

        assert_eq!(texts(&diagram), ["1. a", "1.1. b", "1.2. c", "1.3. d"]);
    }

    #[test]
    fn a_block_title_holds_its_keyword_and_its_condition() {
        let diagram = Diagram::read("sequenceDiagram\nloop Every minute\nA->>B: ping\nend\nrect rgb(0, 0, 0)\nend\n").unwrap();

        assert_eq!(diagram.events[0], Event::Open { block: Block::Loop, title: "loop [Every minute]".to_owned() });
        assert_eq!(diagram.events[3], Event::Open { block: Block::Rect, title: String::new() });
    }

    #[test]
    fn a_created_participant_is_drawn_at_the_first_message_involving_it() {
        let diagram = Diagram::read("sequenceDiagram\nA->>B: hi\ncreate participant C\nNote over A: wait\nB->>C: new\n").unwrap();

        assert_eq!(diagram.participants[2].created, Some(2));
    }

    #[test]
    fn figures_follow_the_participant_type() {
        let diagram =
            Diagram::read("sequenceDiagram\nactor A\nparticipant D@{ \"type\": \"database\" }\nparticipant Q@{ \"type\": \"queue\" }\n")
                .unwrap();

        let figures: Vec<Figure> = diagram.participants.iter().map(|participant| participant.figure).collect();
        assert_eq!(figures, [Figure::Person, Figure::Database, Figure::Box]);
    }

    #[test]
    fn an_activation_of_a_participant_not_there_draws_nothing() {
        let diagram = Diagram::read("sequenceDiagram\nactivate X\nA->>B: hi\n").unwrap();

        assert_eq!(diagram.events.len(), 1);
    }
}
