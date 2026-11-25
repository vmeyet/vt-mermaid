//! Where each edge best leaves its source and joins its target: the side facing the other end, and a cell of that
//! side for each edge, in the order of where they go, so edges of one side neither cross nor crowd the middle.

use std::collections::BTreeMap;

use vt_canvas::{Direction, Point, Rect};

use super::diagram::{Diagram, End};
use super::layout::Placement;
use super::route::opposite;

/// A border cell an edge goes through and the direction it travels there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Join {
    pub(super) heading: Direction,
    pub(super) cell: Point,
    /// The trunk the edge shares with the other edges through this cell, when its side has no room for each.
    pub(super) trunk: Option<u16>,
}

/// The joins an edge is best drawn with; the search may settle for others at a cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Joins {
    pub(super) exit: Join,
    pub(super) entry: Join,
}

/// An edge's end waiting for a cell on a side of its block.
#[derive(Debug, Clone, Copy)]
struct Waiting {
    edge: usize,
    leaves: bool,
    /// The middle of the block at the other end, along the side.
    towards: u16,
}

/// The joins of every edge, in the edges' order.
pub(super) fn joins(diagram: &Diagram, placement: &Placement) -> Vec<Joins> {
    let rect = |end: End| match end {
        End::Node(index) => placement.nodes[index],
        End::Group(index) => placement.groups[index],
    };
    let mut joins: Vec<Joins> = Vec::with_capacity(diagram.edges.len());
    let mut sides: BTreeMap<(usize, usize, u8), Vec<Waiting>> = BTreeMap::new();
    for (index, edge) in diagram.edges.iter().enumerate() {
        let (from, to) = (rect(edge.from), rect(edge.to));
        let flow = diagram.flow_between(edge.from, edge.to);
        let (exit, entry) = if edge.from == edge.to { (flow, flow) } else { (heading(from, to, flow), opposite(heading(to, from, flow))) };
        joins.push(Joins {
            exit: Join { heading: exit, cell: middle(from, exit), trunk: None },
            entry: Join { heading: entry, cell: middle(to, opposite(entry)), trunk: None },
        });
        if edge.from != edge.to {
            sides.entry(key(edge.from, exit)).or_default().push(Waiting { edge: index, leaves: true, towards: along(exit, centre(to)) });
            sides.entry(key(edge.to, opposite(entry))).or_default().push(Waiting {
                edge: index,
                leaves: false,
                towards: along(entry, centre(from)),
            });
        }
    }
    let mut trunks = 0;
    for ((.., side), waiting) in sides {
        let side = SIDES[usize::from(side)];
        let mut waiting = waiting;
        waiting.sort_by_key(|end| (end.towards, end.edge));
        let block = |end: &Waiting| if end.leaves { rect(diagram.edges[end.edge].from) } else { rect(diagram.edges[end.edge].to) };
        let Some(block) = waiting.first().map(block) else { continue };
        let units = units(diagram, &waiting, Side { rect: block, side }, placement.shares_ways);
        let towards: Vec<u16> = units.iter().map(|unit| unit.towards).collect();
        for (unit, cell) in units.iter().zip(cells(Side { rect: block, side }, &towards)) {
            let trunk = (unit.ends.len() > 1).then_some(trunks);
            trunks += u16::from(trunk.is_some());
            for end in &unit.ends {
                let join = if end.leaves { &mut joins[end.edge].exit } else { &mut joins[end.edge].entry };
                (join.cell, join.trunk) = (cell, trunk);
            }
        }
    }
    in_line_with_labels(joins, diagram, placement)
}

/// The joins with those of an edge going down or up through the room kept for its label moved under that room, as
/// near its middle as the side has a free cell: labels side by side then each stand on their edge.
fn in_line_with_labels(joins: Vec<Joins>, diagram: &Diagram, placement: &Placement) -> Vec<Joins> {
    let rect = |end: End| match end {
        End::Node(index) => placement.nodes[index],
        End::Group(index) => placement.groups[index],
    };
    let mut taken: Vec<Point> = joins.iter().flat_map(|joins| [joins.exit.cell, joins.entry.cell]).collect();
    let mut moved = joins;
    for (index, edge) in diagram.edges.iter().enumerate() {
        let Some(room) = placement.labels[index] else { continue };
        let middle = room.x + room.width.saturating_sub(1) / 2;
        for (block, leaves) in [(rect(edge.from), true), (rect(edge.to), false)] {
            let join = if leaves { moved[index].exit } else { moved[index].entry };
            if join.trunk.is_some() || matches!(join.heading, Direction::Left | Direction::Right) {
                continue;
            }
            let side = block.x + 1..(block.x + block.width).saturating_sub(1);
            let mut under: Vec<u16> = (room.x..room.x + room.width).filter(|x| side.contains(x)).collect();
            under.sort_by_key(|x| x.abs_diff(middle));
            let free = |x: &u16| *x == join.cell.x || !taken.contains(&Point::new(*x, join.cell.y));
            if let Some(x) = under.into_iter().find(free) {
                let cell = Point::new(x, join.cell.y);
                taken.push(cell);
                if leaves { moved[index].exit.cell = cell } else { moved[index].entry.cell = cell }
            }
        }
    }
    moved
}

/// A side of a block.
#[derive(Debug, Clone, Copy)]
struct Side {
    rect: Rect,
    side: Direction,
}

impl Side {
    /// The middle cell along the side, then the first and the last a join may take: never a corner.
    fn room(self) -> (i32, i32, i32) {
        let Self { rect, side } = self;
        if matches!(side, Direction::Up | Direction::Down) {
            (i32::from(centre(rect).x), i32::from(rect.x) + 1, i32::from(rect.x + rect.width) - 2)
        } else {
            (i32::from(centre(rect).y), i32::from(rect.y) + 1, i32::from(rect.y + rect.height) - 2)
        }
    }
}

/// Ends given one cell: an end alone, or the ends sharing a trunk.
#[derive(Debug, Clone)]
struct Unit {
    ends: Vec<Waiting>,
    towards: u16,
}

/// Each end a unit while the side has a cell for each and the layout does not ask them to share; else the ends that draw alike, all leaving
/// or all arriving with one kind of line and arrow, share a unit, so they share a trunk.
fn units(diagram: &Diagram, waiting: &[Waiting], side: Side, shares_ways: bool) -> Vec<Unit> {
    let alone = |end: &Waiting| Unit { ends: vec![*end], towards: end.towards };
    let towards: Vec<u16> = waiting.iter().map(|end| end.towards).collect();
    if !shares_ways && spread(side, &towards).is_some() {
        return waiting.iter().map(alone).collect();
    }
    let alike = |end: &Waiting| {
        let edge = &diagram.edges[end.edge];
        (!edge.arrow_start).then_some((end.leaves, edge.kind, edge.arrow_end))
    };
    let mut units: Vec<Unit> = Vec::new();
    for end in waiting {
        let like = alike(end);
        match units.iter_mut().find(|unit| like.is_some() && alike(&unit.ends[0]) == like) {
            Some(unit) => unit.ends.push(*end),
            None => units.push(alone(end)),
        }
    }
    let middle = side.room().0;
    for unit in &mut units {
        let straight = unit.ends.iter().find(|end| i32::from(end.towards) == middle);
        unit.towards = straight.unwrap_or(&unit.ends[unit.ends.len() / 2]).towards;
    }
    units.sort_by_key(|unit| unit.towards);
    units
}

const SIDES: [Direction; 4] = [Direction::Up, Direction::Right, Direction::Down, Direction::Left];

fn key(end: End, side: Direction) -> (usize, usize, u8) {
    let side = SIDES.iter().position(|&known| known == side).unwrap_or(0) as u8;
    match end {
        End::Node(index) => (0, index, side),
        End::Group(index) => (1, index, side),
    }
}

/// The way out of `rect` towards `other`: along `flow` when `other` lies wholly before or after it, else across.
fn heading(rect: Rect, other: Rect, flow: Direction) -> Direction {
    let below = other.y >= rect.y + rect.height;
    let above = other.y + other.height <= rect.y;
    let right = other.x >= rect.x + rect.width;
    let left = other.x + other.width <= rect.x;
    let down_or_up = if below || (!above && centre(other).y > centre(rect).y) { Direction::Down } else { Direction::Up };
    let sideways = if right || (!left && centre(other).x > centre(rect).x) { Direction::Right } else { Direction::Left };
    let along_the_flow = if matches!(flow, Direction::Down | Direction::Up) { below || above } else { !(right || left) };
    if along_the_flow { down_or_up } else { sideways }
}

fn centre(rect: Rect) -> Point {
    Point::new(rect.x + (rect.width - 1) / 2, rect.y + (rect.height - 1) / 2)
}

/// The coordinate of `point` that changes along a side crossed going `heading`.
fn along(heading: Direction, point: Point) -> u16 {
    if matches!(heading, Direction::Down | Direction::Up) { point.x } else { point.y }
}

/// The middle cell of the `side` of `rect`.
fn middle(rect: Rect, side: Direction) -> Point {
    let centre = centre(rect);
    match side {
        Direction::Up => Point::new(centre.x, rect.y),
        Direction::Down => Point::new(centre.x, rect.y + rect.height - 1),
        Direction::Left => Point::new(rect.x, centre.y),
        Direction::Right => Point::new(rect.x + rect.width - 1, centre.y),
    }
}

/// A cell of the side for each unit, given in order by where it goes: the unit going straight across from the
/// middle gets the middle and the others stand on each side of it, else all are spread round the middle; two cells
/// apart while the side has room, never on a corner.
fn cells(side: Side, towards: &[u16]) -> Vec<Point> {
    let (centre, first, last) = side.room();
    let places = spread(side, towards).unwrap_or_else(|| {
        let places = spread_from(centre, towards, Spread { step: 1, straight: false });
        places.into_iter().map(|place| place.clamp(first.min(last), last.max(first))).collect()
    });
    let middle = middle(side.rect, side.side);
    places
        .into_iter()
        .map(|place| {
            if matches!(side.side, Direction::Up | Direction::Down) {
                Point::new(place as u16, middle.y)
            } else {
                Point::new(middle.x, place as u16)
            }
        })
        .collect()
}

/// How units are spread along a side.
#[derive(Debug, Clone, Copy)]
struct Spread {
    /// Cells from one unit to the next.
    step: i32,
    /// Whether the unit going straight across from the middle gets the middle.
    straight: bool,
}

/// The places of the units when the side has room for each: two cells apart first, the unit going straight on the
/// middle first.
fn spread(side: Side, towards: &[u16]) -> Option<Vec<i32>> {
    let (middle, first, last) = side.room();
    let spreads = [(2, true), (2, false), (1, true), (1, false)].map(|(step, straight)| Spread { step, straight });
    spreads
        .into_iter()
        .map(|spread| spread_from(middle, towards, spread))
        .find(|places| places.first().is_some_and(|&place| first <= place) && places.last().is_some_and(|&place| place <= last))
}

fn spread_from(middle: i32, towards: &[u16], spread: Spread) -> Vec<i32> {
    let count = towards.len() as i32;
    let straight = towards.iter().position(|&towards| spread.straight && i32::from(towards) == middle).map(|index| index as i32);
    let offset = |index: i32| match straight {
        Some(straight) => (index - straight) * spread.step,
        None => ((2 * index - (count - 1)) * spread.step).div_euclid(2),
    };
    (0..count).map(|index| middle + offset(index)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flowchart;
    use crate::graph::layout::{self, Fit};

    const BLOCK: Rect = Rect::new(10, 10, 9, 3);

    fn joins_of(source: &str, fit: Fit) -> Vec<Joins> {
        let diagram = flowchart::read(source).unwrap();
        joins(&diagram, &layout::place(&diagram, fit))
    }

    #[test]
    fn edges_leave_through_the_side_facing_their_target_each_through_its_own_cell() {
        let joins = joins_of("flowchart TB\n  A[Start here] --> B & C", Fit::Loose);

        let exits: Vec<(Direction, u16, Option<u16>)> =
            joins.iter().map(|joins| (joins.exit.heading, joins.exit.cell.x, joins.exit.trunk)).collect();
        assert_eq!(exits, [(Direction::Down, 8, None), (Direction::Down, 10, None)]);
        assert!(joins.iter().all(|joins| joins.entry.heading == Direction::Down));
    }

    #[test]
    fn edges_on_a_side_without_a_cell_for_each_share_a_trunk() {
        let joins = joins_of("flowchart LR\n  A --> C\n  B --> C\n  B -.-> C", Fit::Loose);

        let entries: Vec<(Point, Option<u16>)> = joins.iter().map(|joins| (joins.entry.cell, joins.entry.trunk)).collect();
        assert_eq!(entries[0], entries[1]);
        assert!(entries[0].1.is_some() && entries[2].1.is_none());
    }

    #[test]
    fn a_layout_that_asks_edges_to_share_gives_one_trunk_to_a_side_with_room() {
        let joins = joins_of("flowchart TB\n  A[Start here] --> B & C", Fit::Folded { columns: 80 });

        assert_eq!(joins[0].exit, joins[1].exit);
        assert!(joins[0].exit.trunk.is_some());
    }

    #[test]
    fn labelled_edges_between_two_blocks_each_join_them_in_line_with_their_label() {
        let source = "flowchart TB\n  A[A first node wide enough for two labels] -->|the first label| B[Another node just as wide as the first]\n  A -->|the second label| B";
        let diagram = flowchart::read(source).unwrap();
        let placement = layout::place(&diagram, Fit::Loose);

        let joins = joins(&diagram, &placement);

        for (joins, room) in joins.iter().zip(&placement.labels) {
            let under = room.unwrap().x..room.unwrap().x + room.unwrap().width;
            assert!(under.contains(&joins.exit.cell.x) && joins.exit.cell.x == joins.entry.cell.x, "{joins:?} {room:?}");
        }
        assert!(joins[0].entry.cell.x + 10 < joins[1].entry.cell.x, "{joins:?}");
    }

    #[test]
    fn an_edge_against_the_flow_leaves_and_arrives_against_it() {
        let joins = joins_of("flowchart TB\n  A --> B --> A", Fit::Loose);

        assert_eq!((joins[1].exit.heading, joins[1].entry.heading), (Direction::Up, Direction::Up));
    }

    #[test]
    fn a_block_wholly_after_along_the_flow_is_reached_along_it() {
        assert_eq!(heading(BLOCK, Rect::new(30, 16, 9, 3), Direction::Down), Direction::Down);
        assert_eq!(heading(BLOCK, Rect::new(30, 4, 9, 3), Direction::Down), Direction::Up);
        assert_eq!(heading(BLOCK, Rect::new(30, 16, 9, 3), Direction::Right), Direction::Right);
        assert_eq!(heading(BLOCK, Rect::new(0, 16, 9, 3), Direction::Right), Direction::Left);
    }

    #[test]
    fn a_block_beside_across_the_flow_is_reached_across_it() {
        assert_eq!(heading(BLOCK, Rect::new(30, 11, 9, 3), Direction::Down), Direction::Right);
        assert_eq!(heading(BLOCK, Rect::new(12, 20, 9, 3), Direction::Right), Direction::Down);
    }

    #[test]
    fn ends_without_one_going_straight_are_spread_round_the_middle() {
        let cells = cells(Side { rect: BLOCK, side: Direction::Down }, &[3, 30]);

        assert_eq!(cells, [Point::new(13, 12), Point::new(15, 12)]);
    }

    #[test]
    fn the_end_going_straight_gets_the_middle_and_the_others_stand_beside_it() {
        let cells = cells(Side { rect: BLOCK, side: Direction::Down }, &[3, 14, 30]);

        assert_eq!(cells, [Point::new(12, 12), Point::new(14, 12), Point::new(16, 12)]);
    }

    #[test]
    fn ends_too_many_to_stand_two_apart_stand_side_by_side_off_the_corners() {
        let cells = cells(Side { rect: BLOCK, side: Direction::Up }, &[1, 2, 3, 4, 5, 6]);

        assert_eq!(cells.iter().map(|cell| cell.x).collect::<Vec<_>>(), [11, 12, 13, 14, 15, 16]);
    }
}
