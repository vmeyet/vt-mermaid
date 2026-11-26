//! Each participant's column: heads apart, then the room each event needs beside or between its participants.

use super::{FRAME_PADDING, GROUP_PADDING, Head};
use crate::label::Label;
use crate::sequence::diagram::{Diagram, Event};
use crate::sequence::parse::Placement;

/// How far an event reaches: from the participant at `low`, `before` columns further left, to the one at `high`,
/// `after` columns further right; and the columns it needs between those two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Reach {
    low: usize,
    before: i32,
    high: usize,
    after: i32,
    between: i32,
}

/// The columns an event needs around the participants it involves.
fn reach(diagram: &Diagram, heads: &[Head], index: usize, event: &Event, label: &Label) -> Option<Reach> {
    let width = i32::from(label.width);
    let note = width + 4;
    match *event {
        Event::Message { from, to, .. } if from == to => {
            Some(Reach { low: from, before: 0, high: from, after: (width + 2).max(4), between: 0 })
        }
        Event::Message { from, to, .. } => {
            let creates = diagram.participants[to].created == Some(index);
            let head = if !creates {
                0
            } else if to > from {
                heads[to].before()
            } else {
                heads[to].after()
            };
            Some(Reach { low: from.min(to), before: 0, high: from.max(to), after: 0, between: (width + 3).max(6) + head })
        }
        Event::Note { first, last, placement, .. } => Some(match placement {
            Placement::RightOf => Reach { low: last, before: 0, high: last, after: note + 1, between: 0 },
            Placement::LeftOf => Reach { low: first, before: note + 1, high: first, after: 0, between: 0 },
            Placement::Over if first == last => Reach { low: first, before: note / 2, high: first, after: note / 2, between: 0 },
            Placement::Over => Reach { low: first, before: 2, high: last, after: 2, between: note - 4 },
        }),
        _ => None,
    }
}

/// Each participant's lifeline column: heads apart by `gap`, then the room each event needs beside or between its
/// participants, a frame's padding for each level of block around it, and a box's around its participants.
pub(super) fn columns(diagram: &Diagram, heads: &[Head], labels: &[Label], gap: i32) -> Vec<i32> {
    let events = diagram.events.iter().zip(labels).enumerate();
    let reaches: Vec<Option<Reach>> = events.map(|(index, (event, label))| reach(diagram, heads, index, event, label)).collect();
    let count = heads.len();
    let mut gaps: Vec<i32> = (1..count)
        .map(|right| {
            let left = right - 1;
            let crosses_group = diagram.participants[left].group != diagram.participants[right].group;
            heads[left].after() + gap + 1 + heads[right].before() + if crosses_group { 2 * GROUP_PADDING + 1 } else { 0 }
        })
        .collect();
    let mut spans: Vec<(usize, usize, i32)> = Vec::new();
    let mut add_reach = |reach: Reach, padding: i32| {
        if reach.low > 0 {
            spans.push((reach.low - 1, reach.low, reach.before + padding + 2));
        }
        if reach.high + 1 < count {
            spans.push((reach.high, reach.high + 1, reach.after + padding + 2));
        }
        if reach.high > reach.low {
            spans.push((reach.low, reach.high, reach.between));
        }
    };
    for reach in reaches.iter().flatten() {
        add_reach(*reach, 0);
    }
    for (reach, depth) in frame_reaches(&diagram.events, &reaches) {
        add_reach(Reach { between: 0, ..reach }, FRAME_PADDING * depth);
    }
    spans.sort_by_key(|&(low, high, _)| high - low);
    for (low, high, need) in spans {
        let deficit = need - gaps[low..high].iter().sum::<i32>();
        if deficit > 0 {
            let widened = spread(&gaps[low..high], deficit);
            gaps.splice(low..high, widened);
        }
    }
    std::iter::once(0)
        .chain(gaps.iter().scan(0, |x, gap| {
            *x += gap;
            Some(*x)
        }))
        .collect()
}

/// `deficit` columns shared out among `gaps`, the last ones taking what does not divide.
fn spread(gaps: &[i32], deficit: i32) -> Vec<i32> {
    let count = i32::try_from(gaps.len()).unwrap_or(i32::MAX).max(1);
    (0..).zip(gaps).map(|(index, gap)| gap + deficit / count + i32::from(index >= count - deficit % count)).collect()
}

/// Each block's reach, the union of its events', and how many levels of block it holds, itself included.
fn frame_reaches(events: &[Event], reaches: &[Option<Reach>]) -> Vec<(Reach, i32)> {
    let mut open: Vec<(Option<Reach>, i32)> = Vec::new();
    let mut closed = Vec::new();
    for (event, reach) in events.iter().zip(reaches) {
        match event {
            Event::Open { .. } => open.push((None, 0)),
            Event::Close => {
                let Some((reach, inside)) = open.pop() else { continue };
                let depth = inside + 1;
                if let Some(parent) = open.last_mut() {
                    parent.1 = parent.1.max(depth);
                    parent.0 = joined(parent.0, reach);
                }
                closed.extend(reach.map(|reach| (reach, depth)));
            }
            _ => {
                for frame in &mut open {
                    frame.0 = joined(frame.0, *reach);
                }
            }
        }
    }
    closed
}

fn joined(one: Option<Reach>, other: Option<Reach>) -> Option<Reach> {
    let (Some(one), Some(other)) = (one, other) else { return one.or(other) };
    let (low, before) = match one.low.cmp(&other.low) {
        std::cmp::Ordering::Less => (one.low, one.before),
        std::cmp::Ordering::Greater => (other.low, other.before),
        std::cmp::Ordering::Equal => (one.low, one.before.max(other.before)),
    };
    let (high, after) = match one.high.cmp(&other.high) {
        std::cmp::Ordering::Greater => (one.high, one.after),
        std::cmp::Ordering::Less => (other.high, other.after),
        std::cmp::Ordering::Equal => (one.high, one.after.max(other.after)),
    };
    Some(Reach { low, before, high, after, between: 0 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::label::Wrap;
    use crate::sequence::layout::{LOOSE, event_label, head};
    use crate::settings::LongWords;

    fn columns_of(body: &str) -> Vec<i32> {
        let diagram = Diagram::read_source(&format!("sequenceDiagram\n{body}")).unwrap();
        let wrap = Wrap::wide(LongWords::Break);
        let heads: Vec<Head> =
            diagram.participants.iter().map(|participant| head(participant.figure, Label::wrapped_as(&participant.label, wrap))).collect();
        let labels: Vec<Label> = diagram.events.iter().map(|event| event_label(event, wrap)).collect();
        columns(&diagram, &heads, &labels, LOOSE)
    }

    #[test]
    fn heads_stand_apart_by_the_gap() {
        assert_eq!(columns_of("participant Alice\nparticipant Bob\n"), [0, 4 + LOOSE + 1 + 3]);
    }

    #[test]
    fn a_long_label_moves_the_receiver_away() {
        assert_eq!(columns_of("A->>B: a label of thirty columns long\n"), [0, 33]);
    }

    #[test]
    fn a_label_spanning_participants_shares_its_room_among_their_gaps() {
        let columns = columns_of("participant A\nparticipant B\nparticipant C\nA->>C: a label of thirty columns long\n");

        assert!(columns[2] >= 33 && columns[1] - columns[0] > 7, "{columns:?}");
    }

    #[test]
    fn shared_out_columns_add_up_to_the_deficit() {
        assert_eq!(spread(&[5, 5, 5], 7), [7, 7, 8]);
    }
}
