//! Layout pixels to grid cells: each group is placed as one block in its parent, so its nodes stay inside it.

use vt_canvas::{Direction, Rect};

use super::diagram::{Diagram, End, PX_PER_COLUMN, PX_PER_ROW};
use super::shape::{self, Size};

/// Empty cells kept around the drawing so edges can go round it.
pub(super) const MARGIN: u16 = 3;
/// Cells between a group's frame and what it holds, on every side.
const GROUP_PADDING: i32 = 2;
/// A frame title sits between `┌─ ` and ` ─┐`.
const TITLE_FRAME: i32 = 6;

/// How much of the room the layout leaves between blocks, from side to side of the screen, the snap keeps.
///
/// The gaps edges need are kept whatever the room, so less room only pulls blocks together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Room {
    Whole,
    Half,
    /// None, and blocks going down side by side past `columns` are folded into as many rows as it takes.
    Folded {
        columns: i32,
    },
}

impl Room {
    fn kept(self, cells: i32) -> i32 {
        match self {
            Self::Whole => cells,
            Self::Half => cells / 2,
            Self::Folded { .. } => 0,
        }
    }

    /// The room inside a group's frame and padding.
    fn inside_group(self) -> Self {
        match self {
            Self::Folded { columns } => Self::Folded { columns: columns - 2 * GROUP_PADDING },
            room => room,
        }
    }
}

/// Where every node and group lands, in grid cells, [`MARGIN`] included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Placement {
    pub(super) nodes: Vec<Rect>,
    pub(super) groups: Vec<Rect>,
    pub(super) size: Size,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Item {
    Node(usize),
    Group(usize),
}

/// A container's content, its children at cells relative to its top-left corner.
#[derive(Debug, Clone, Default)]
struct Arrangement {
    width: i32,
    height: i32,
    children: Vec<Child>,
}

#[derive(Debug, Clone)]
struct Child {
    item: Item,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    /// A group's own content; empty for a node.
    content: Arrangement,
}

/// A block to place: where the layout puts it and how big it is, in cells.
#[derive(Debug, Clone, Copy)]
struct Block {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

/// Snaps the layout to cells, keeping the order of ranks and the gaps edges need.
pub(super) fn place(diagram: &Diagram, room: Room) -> Placement {
    let root = arrange(diagram, None, room);
    let margin = i32::from(MARGIN);
    let mut placement = Placement {
        nodes: vec![Rect::new(0, 0, 0, 0); diagram.nodes.len()],
        groups: vec![Rect::new(0, 0, 0, 0); diagram.groups.len()],
        size: Size { width: to_cells(root.width + 2 * margin), height: to_cells(root.height + 2 * margin) },
    };
    for (item, rect) in flatten(&root, margin, margin) {
        match item {
            Item::Node(index) => placement.nodes[index] = rect,
            Item::Group(index) => placement.groups[index] = rect,
        }
    }
    placement
}

fn arrange(diagram: &Diagram, container: Option<usize>, room: Room) -> Arrangement {
    let nodes = (0..diagram.nodes.len()).filter(|&index| diagram.nodes[index].group == container).map(Item::Node);
    let groups = (0..diagram.groups.len()).filter(|&index| diagram.groups[index].parent == container).map(Item::Group);
    let items: Vec<(Item, Arrangement)> = nodes
        .chain(groups)
        .map(|item| match item {
            Item::Node(_) => (item, Arrangement::default()),
            Item::Group(index) => (item, arrange(diagram, Some(index), room.inside_group())),
        })
        .collect();
    let flow = container.map_or(diagram.flow, |group| diagram.groups[group].flow);
    let blocks: Vec<Block> = items.iter().map(|(item, content)| block(diagram, *item, content, flow)).collect();
    // The layout already puts a rank after the blocks its edges come from; only a pulled rank has to be told.
    let joined = if room == Room::Whole { Vec::new() } else { joined(diagram, container, &items) };
    let corners = separate(&blocks, &joined, Fit { flow, gaps: gaps(diagram, flow), room });
    let (left, top) = (corners.iter().map(|&(x, _)| x).min().unwrap_or(0), corners.iter().map(|&(_, y)| y).min().unwrap_or(0));
    let children: Vec<Child> = items
        .into_iter()
        .zip(blocks.iter().zip(&corners))
        .map(|((item, content), (block, &(x, y)))| Child {
            item,
            x: x - left,
            y: y - top,
            width: block.width,
            height: block.height,
            content,
        })
        .collect();
    let width = children.iter().map(|child| child.x + child.width).max().unwrap_or(0);
    let height = children.iter().map(|child| child.y + child.height).max().unwrap_or(0);
    Arrangement { width, height, children }
}

/// The pairs of `items` an edge joins, whatever its ends' depth inside them.
fn joined(diagram: &Diagram, container: Option<usize>, items: &[(Item, Arrangement)]) -> Vec<(usize, usize)> {
    let position = |end: End| {
        let held = item_holding(diagram, container, end)?;
        items.iter().position(|(item, _)| *item == held)
    };
    diagram.edges.iter().filter_map(|edge| Some((position(edge.from)?, position(edge.to)?))).filter(|(from, to)| from != to).collect()
}

/// The child of `container` that is `end` or holds it.
fn item_holding(diagram: &Diagram, container: Option<usize>, end: End) -> Option<Item> {
    let parent = |item: Item| match item {
        Item::Node(node) => diagram.nodes[node].group,
        Item::Group(group) => diagram.groups[group].parent,
    };
    let start = match end {
        End::Node(node) => Item::Node(node),
        End::Group(group) => Item::Group(group),
    };
    std::iter::successors(Some(start), |&item| parent(item).map(Item::Group))
        .take(diagram.groups.len() + 1)
        .find(|&item| parent(item) == container)
}

/// A node keeps the layout's rank start along the flow and its middle across it, so nodes the layout centres on one
/// line stay centred whatever the parity of their widths.
fn block(diagram: &Diagram, item: Item, content: &Arrangement, flow: Direction) -> Block {
    match item {
        Item::Node(index) => {
            let node = &diagram.nodes[index];
            let size = shape::size(node.shape, &node.label);
            let (width, height) = (f32::from(size.width), f32::from(size.height));
            let (x, y) = if matches!(flow, Direction::Down | Direction::Up) {
                (column(node.x + width * PX_PER_COLUMN / 2.0) - i32::from(size.width / 2), row(node.y))
            } else {
                (column(node.x), row(node.y + height * PX_PER_ROW / 2.0) - i32::from(size.height / 2))
            };
            Block { x, y, width: i32::from(size.width), height: i32::from(size.height) }
        }
        Item::Group(index) => {
            let (x, y) = top_left(diagram, index).unwrap_or((0, 0));
            let title = i32::from(diagram.groups[index].title.width) + TITLE_FRAME;
            Block {
                x: x - GROUP_PADDING,
                y: y - GROUP_PADDING,
                width: (content.width + 2 * GROUP_PADDING).max(title),
                height: content.height + 2 * GROUP_PADDING,
            }
        }
    }
}

/// The top-left cell of the nodes a group holds, at any depth.
fn top_left(diagram: &Diagram, group: usize) -> Option<(i32, i32)> {
    let inside = |node: Option<usize>| {
        std::iter::successors(node, |&index| diagram.groups[index].parent).take(diagram.groups.len() + 1).any(|index| index == group)
    };
    let corners: Vec<(i32, i32)> =
        diagram.nodes.iter().filter(|node| inside(node.group)).map(|node| (column(node.x), row(node.y))).collect();
    Some((corners.iter().map(|&(x, _)| x).min()?, corners.iter().map(|&(_, y)| y).min()?))
}

fn flatten(arrangement: &Arrangement, x: i32, y: i32) -> Vec<(Item, Rect)> {
    arrangement
        .children
        .iter()
        .flat_map(|child| {
            let (left, top) = (x + child.x, y + child.y);
            let own = (child.item, Rect::new(to_cells(left), to_cells(top), to_cells(child.width), to_cells(child.height)));
            let inner = flatten(&child.content, left + GROUP_PADDING, top + GROUP_PADDING);
            std::iter::once(own).chain(inner)
        })
        .collect()
}

/// How blocks are set apart: the way their ranks go, the gaps between them and the room kept beyond those.
#[derive(Debug, Clone, Copy)]
struct Fit {
    flow: Direction,
    gaps: Gaps,
    room: Room,
}

/// Top-left corners for `blocks` such that any two are apart by a rank gap along the flow or a sibling gap across it.
///
/// Blocks the layout puts on one rank stay on one row (or column), and a rank comes after the blocks in its way and
/// those `joined` to it by an edge.
fn separate(blocks: &[Block], joined: &[(usize, usize)], fit: Fit) -> Vec<(i32, i32)> {
    let Fit { flow, gaps, room } = fit;
    let vertical = matches!(flow, Direction::Down | Direction::Up);
    let spans: Vec<Span> = blocks
        .iter()
        .map(|block| {
            if vertical {
                Span { along: block.y, across: block.x, length: block.height, breadth: block.width }
            } else {
                Span { along: block.x, across: block.y, length: block.width, breadth: block.height }
            }
        })
        .collect();
    let spans = match room {
        Room::Folded { columns } if vertical => folded(&spans, gaps, columns),
        _ => spans,
    };
    let starts: Vec<Start> = spans.iter().map(|span| span.start(room, vertical)).collect();
    let across = spread_across(&spans, &starts, gaps.sibling);
    let along = push_along(&spans, &starts, &across, gaps, joined);
    along.into_iter().zip(across).map(|(along, across)| if vertical { (across, along) } else { (along, across) }).collect()
}

/// Cells kept between ranks, and between blocks of one rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Gaps {
    rank: i32,
    sibling: i32,
}

/// Room for a bend and an arrow; across a horizontal flow a rank gap also holds the widest edge label and its arrow.
fn gaps(diagram: &Diagram, flow: Direction) -> Gaps {
    if matches!(flow, Direction::Down | Direction::Up) {
        Gaps { rank: 3, sibling: 4 }
    } else {
        let widest = diagram.edges.iter().filter_map(|edge| edge.label.as_ref()).map(|label| i32::from(label.width)).max().unwrap_or(0);
        Gaps { rank: (widest + 4).max(6), sibling: 1 }
    }
}

/// A block seen from its flow: `along` the ranks, `across` them.
#[derive(Debug, Clone, Copy)]
struct Span {
    along: i32,
    across: i32,
    length: i32,
    breadth: i32,
}

/// Where a block starts before it is moved away from the others.
#[derive(Debug, Clone, Copy)]
struct Start {
    along: i32,
    across: i32,
}

impl Span {
    /// The layout's place, pulled towards the left of the screen as `room` asks: a rank going down keeps its blocks'
    /// middles in proportion, so a chain stays on one line, until no room leaves every block on the left; a rank
    /// going sideways keeps its start in proportion.
    fn start(self, room: Room, vertical: bool) -> Start {
        if !vertical {
            return Start { along: room.kept(self.along), across: self.across };
        }
        let across = match room {
            Room::Folded { .. } => 0,
            Room::Whole | Room::Half => room.kept(self.across + self.breadth / 2) - self.breadth / 2,
        };
        Start { along: self.along, across }
    }
}

/// Rows no wider than `columns`: as long as a block ends past them, it moves under the blocks it stands beside, and
/// takes along the rest of its rank and every later rank.
fn folded(spans: &[Span], gaps: Gaps, columns: i32) -> Vec<Span> {
    let mut folded = spans.to_vec();
    for _ in 0..spans.len() {
        let starts: Vec<Start> = folded.iter().map(|span| Start { along: span.along, across: 0 }).collect();
        let across = spread_across(&folded, &starts, gaps.sibling);
        let place = |index: usize| (folded[index].along, folded[index].across);
        let too_wide = |&index: &usize| across[index] > 0 && across[index] + folded[index].breadth > columns;
        let Some(wide) = (0..folded.len()).filter(too_wide).min_by_key(|&index| place(index)) else {
            break;
        };
        let beside = (0..folded.len()).filter(|&other| across[other] < across[wide] && side_by_side(folded[wide], folded[other]));
        let under = beside.map(|other| folded[other].along + folded[other].length).max().unwrap_or(0) + gaps.rank;
        let (moved, from) = (under - folded[wide].along, place(wide));
        folded = folded
            .iter()
            .map(|&span| if (span.along, span.across) >= from { Span { along: span.along + moved, ..span } } else { span })
            .collect();
    }
    folded
}

/// Blocks side by side move apart across the flow, in the layout's order; other blocks are left to [`push_along`],
/// so a tall block never pushes the next rank sideways.
fn spread_across(spans: &[Span], starts: &[Start], gap: i32) -> Vec<i32> {
    let mut order: Vec<usize> = (0..spans.len()).collect();
    order.sort_by_key(|&index| (spans[index].across, spans[index].along));
    let mut across = vec![0; spans.len()];
    for (position, &index) in order.iter().enumerate() {
        let span = spans[index];
        let pushed = order[..position]
            .iter()
            .filter(|&&other| side_by_side(span, spans[other]))
            .map(|&other| across[other] + spans[other].breadth + gap);
        across[index] = pushed.fold(starts[index].across, i32::max);
    }
    across
}

/// Ranks in the layout's order, each moved along the flow until it clears every earlier rank near it across and
/// every earlier block an edge in `joined` joins it to.
fn push_along(spans: &[Span], starts: &[Start], across: &[i32], gaps: Gaps, joined: &[(usize, usize)]) -> Vec<i32> {
    let joins = |index: usize, earlier: usize| joined.contains(&(index, earlier)) || joined.contains(&(earlier, index));
    let mut order: Vec<usize> = (0..spans.len()).collect();
    order.sort_by_key(|&index| (spans[index].along, spans[index].across));
    let mut along = vec![0; spans.len()];
    let mut shift = 0;
    let mut start = 0;
    while start < order.len() {
        let (rank, kept) = (spans[order[start]].along, starts[order[start]].along);
        let end = order[start..].iter().position(|&index| spans[index].along != rank).map_or(order.len(), |offset| start + offset);
        for &index in &order[start..end] {
            for &earlier in &order[..start] {
                let in_the_way = overlap(across[index], spans[index].breadth, across[earlier], spans[earlier].breadth, gaps.sibling);
                if in_the_way || (joins(index, earlier) && !side_by_side(spans[index], spans[earlier])) {
                    shift = shift.max(along[earlier] + spans[earlier].length + gaps.rank - kept);
                }
            }
        }
        for &index in &order[start..end] {
            along[index] = kept + shift;
        }
        start = end;
    }
    along
}

/// On one rank, or one within the other's extent along the flow, as a node beside a group.
fn side_by_side(span: Span, other: Span) -> bool {
    let within = |inner: Span, outer: Span| outer.along <= inner.along && inner.along + inner.length <= outer.along + outer.length;
    span.along == other.along || within(span, other) || within(other, span)
}

/// Whether two spans come within `gap` of each other.
fn overlap(start: i32, length: i32, other_start: i32, other_length: i32, gap: i32) -> bool {
    start < other_start + other_length + gap && other_start < start + length + gap
}

fn column(px: f32) -> i32 {
    (px / PX_PER_COLUMN).round() as i32
}

fn row(px: f32) -> i32 {
    (px / PX_PER_ROW).round() as i32
}

fn to_cells(value: i32) -> u16 {
    u16::try_from(value.max(0)).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOWN: Gaps = Gaps { rank: 3, sibling: 4 };

    const SIDEWAYS: Gaps = Gaps { rank: 6, sibling: 1 };

    fn block(x: i32, y: i32, width: i32, height: i32) -> Block {
        Block { x, y, width, height }
    }

    fn down(room: Room) -> Fit {
        Fit { flow: Direction::Down, gaps: DOWN, room }
    }

    fn sideways(room: Room) -> Fit {
        Fit { flow: Direction::Right, gaps: SIDEWAYS, room }
    }

    #[test]
    fn blocks_already_apart_stay_where_the_layout_puts_them() {
        let blocks = [block(0, 0, 5, 3), block(20, 0, 5, 3), block(0, 10, 5, 3)];

        assert_eq!(separate(&blocks, &[], down(Room::Whole)), [(0, 0), (20, 0), (0, 10)]);
    }

    #[test]
    fn blocks_of_one_rank_move_apart_across_the_flow() {
        let blocks = [block(0, 0, 10, 3), block(6, 0, 5, 3)];

        assert_eq!(separate(&blocks, &[], down(Room::Whole)), [(0, 0), (14, 0)]);
    }

    #[test]
    fn a_rank_too_close_to_the_one_before_moves_along_with_every_later_rank() {
        let blocks = [block(0, 0, 5, 3), block(0, 4, 5, 3), block(30, 4, 5, 3), block(0, 12, 5, 3)];

        assert_eq!(separate(&blocks, &[], down(Room::Whole)), [(0, 0), (0, 6), (30, 6), (0, 14)]);
    }

    #[test]
    fn a_block_within_a_bigger_blocks_extent_moves_across_it() {
        let group = block(4, 0, 20, 21);
        let beside = block(15, 21, 7, 3);

        assert_eq!(separate(&[group, beside], &[], sideways(Room::Whole)), [(4, 0), (15, 22)]);
    }

    #[test]
    fn half_the_room_pulls_the_middles_of_a_ranks_blocks_halfway_to_the_left() {
        let blocks = [block(0, 0, 4, 3), block(40, 0, 4, 3)];

        assert_eq!(separate(&blocks, &[], down(Room::Half)), [(-1, 0), (19, 0)]);
    }

    #[test]
    fn no_room_packs_each_rank_on_the_left_a_sibling_gap_apart() {
        let blocks = [block(10, 0, 5, 3), block(40, 0, 5, 3), block(30, 10, 5, 3)];

        assert_eq!(separate(&blocks, &[], down(Room::Folded { columns: 100 })), [(0, 0), (9, 0), (0, 10)]);
    }

    #[test]
    fn a_rank_wider_than_the_columns_is_folded_and_moves_later_ranks_along() {
        let blocks = [block(0, 0, 10, 3), block(20, 0, 10, 3), block(40, 0, 10, 3), block(0, 10, 10, 3)];

        assert_eq!(separate(&blocks, &[], down(Room::Folded { columns: 25 })), [(0, 0), (14, 0), (0, 6), (0, 16)]);
    }

    #[test]
    fn a_block_beside_a_taller_one_moves_under_it_when_both_are_too_wide() {
        let (tall, beside) = (block(0, 0, 20, 12), block(24, 6, 10, 3));

        assert_eq!(separate(&[tall, beside], &[], down(Room::Folded { columns: 34 })), [(0, 0), (24, 6)]);
        assert_eq!(separate(&[tall, beside], &[], down(Room::Folded { columns: 33 })), [(0, 0), (0, 15)]);
    }

    #[test]
    fn a_block_wider_than_the_columns_stays_alone_on_its_row() {
        let blocks = [block(0, 0, 30, 3), block(40, 0, 30, 3)];

        assert_eq!(separate(&blocks, &[], down(Room::Folded { columns: 20 })), [(0, 0), (0, 6)]);
    }

    #[test]
    fn a_rank_as_wide_as_the_columns_is_not_folded() {
        let blocks = [block(0, 0, 10, 3), block(20, 0, 11, 3)];

        assert_eq!(separate(&blocks, &[], down(Room::Folded { columns: 25 })), [(0, 0), (14, 0)]);
    }

    #[test]
    fn a_sideways_rank_pulled_back_stays_a_rank_gap_after_the_block_an_edge_joins_it_to() {
        let blocks = [block(0, 0, 5, 3), block(30, 10, 5, 3)];
        let no_room = sideways(Room::Folded { columns: 100 });

        assert_eq!(separate(&blocks, &[], no_room), [(0, 0), (0, 10)]);
        assert_eq!(separate(&blocks, &[(0, 1)], no_room), [(0, 0), (11, 10)]);
    }

    #[test]
    fn an_edge_into_a_nested_group_joins_the_outermost_group_holding_its_end() {
        let diagram = Diagram::read("flowchart TB\n  subgraph G\n    subgraph H\n      A\n    end\n  end\n  B --> A").unwrap();
        let a = diagram.nodes.iter().position(|node| node.id == "A").unwrap();
        let (g, h) = (0, 1);

        assert_eq!(item_holding(&diagram, None, End::Node(a)), Some(Item::Group(g)));
        assert_eq!(item_holding(&diagram, Some(g), End::Node(a)), Some(Item::Group(h)));
        assert_eq!(item_holding(&diagram, Some(h), End::Node(a)), Some(Item::Node(a)));
        assert_eq!(item_holding(&diagram, Some(h), End::Group(g)), None);
    }

    #[test]
    fn a_folded_group_leaves_its_frame_and_padding_out_of_its_columns() {
        assert_eq!(Room::Folded { columns: 40 }.inside_group(), Room::Folded { columns: 36 });
        assert_eq!(Room::Half.inside_group(), Room::Half);
    }

    #[test]
    fn a_horizontal_rank_gap_holds_the_widest_edge_label() {
        let diagram = Diagram::read("flowchart LR\n  A -->|a fairly long label| B").unwrap();

        assert_eq!(gaps(&diagram, Direction::Right), Gaps { rank: 23, sibling: 1 });
        assert_eq!(gaps(&diagram, Direction::Down), DOWN);
    }

    #[test]
    fn a_group_wraps_its_nodes_with_a_padding_and_its_title() {
        let diagram = Diagram::read("flowchart TB\n  subgraph Outer [A rather long title]\n    A --> B\n  end").unwrap();

        let placement = place(&diagram, Room::Whole);

        let (group, nodes) = (placement.groups[0], &placement.nodes);
        assert_eq!(group.width, 19 + 6);
        assert!(
            nodes.iter().all(|node| node.x >= group.x + 2 && node.y >= group.y + 2 && node.y + node.height + 2 <= group.y + group.height)
        );
    }
}
