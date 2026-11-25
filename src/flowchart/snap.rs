//! Layout pixels to grid cells: each group is placed as one block in its parent, so its nodes stay inside it.

use vt_canvas::{Direction, Rect};

use super::diagram::{Diagram, PX_PER_COLUMN, PX_PER_ROW};
use super::shape::{self, Size};

/// Empty cells kept around the drawing so edges can go round it.
pub(super) const MARGIN: u16 = 3;
/// Cells between a group's frame and what it holds, on every side.
const GROUP_PADDING: i32 = 2;
/// A frame title sits between `┌─ ` and ` ─┐`.
const TITLE_FRAME: i32 = 6;

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
pub(super) fn place(diagram: &Diagram) -> Placement {
    let root = arrange(diagram, None);
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

fn arrange(diagram: &Diagram, container: Option<usize>) -> Arrangement {
    let nodes = (0..diagram.nodes.len()).filter(|&index| diagram.nodes[index].group == container).map(Item::Node);
    let groups = (0..diagram.groups.len()).filter(|&index| diagram.groups[index].parent == container).map(Item::Group);
    let items: Vec<(Item, Arrangement)> = nodes
        .chain(groups)
        .map(|item| match item {
            Item::Node(_) => (item, Arrangement::default()),
            Item::Group(index) => (item, arrange(diagram, Some(index))),
        })
        .collect();
    let flow = container.map_or(diagram.flow, |group| diagram.groups[group].flow);
    let blocks: Vec<Block> = items.iter().map(|(item, content)| block(diagram, *item, content, flow)).collect();
    let corners = separate(&blocks, flow, gaps(diagram, flow));
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

/// Top-left corners for `blocks` such that any two are apart by a rank gap along `flow` or a sibling gap across it.
///
/// Blocks the layout puts on one rank stay on one row (or column); a rank only moves further along the flow.
fn separate(blocks: &[Block], flow: Direction, gaps: Gaps) -> Vec<(i32, i32)> {
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
    let across = spread_across(&spans, gaps.sibling);
    let along = push_along(&spans, &across, gaps);
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

/// Blocks side by side move apart across the flow, in the layout's order; other blocks are left to [`push_along`],
/// so a tall block never pushes the next rank sideways.
fn spread_across(spans: &[Span], gap: i32) -> Vec<i32> {
    let mut order: Vec<usize> = (0..spans.len()).collect();
    order.sort_by_key(|&index| (spans[index].across, spans[index].along));
    let mut across = vec![0; spans.len()];
    for (position, &index) in order.iter().enumerate() {
        let span = spans[index];
        let pushed = order[..position]
            .iter()
            .filter(|&&other| side_by_side(span, spans[other]))
            .map(|&other| across[other] + spans[other].breadth + gap);
        across[index] = pushed.fold(span.across, i32::max);
    }
    across
}

/// Ranks in the layout's order, each moved along the flow until it clears every earlier rank near it across.
fn push_along(spans: &[Span], across: &[i32], gaps: Gaps) -> Vec<i32> {
    let mut order: Vec<usize> = (0..spans.len()).collect();
    order.sort_by_key(|&index| (spans[index].along, spans[index].across));
    let mut along = vec![0; spans.len()];
    let mut shift = 0;
    let mut start = 0;
    while start < order.len() {
        let rank = spans[order[start]].along;
        let end = order[start..].iter().position(|&index| spans[index].along != rank).map_or(order.len(), |offset| start + offset);
        for &index in &order[start..end] {
            for &earlier in &order[..start] {
                if overlap(across[index], spans[index].breadth, across[earlier], spans[earlier].breadth, gaps.sibling) {
                    shift = shift.max(along[earlier] + spans[earlier].length + gaps.rank - rank);
                }
            }
        }
        for &index in &order[start..end] {
            along[index] = rank + shift;
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

    fn block(x: i32, y: i32, width: i32, height: i32) -> Block {
        Block { x, y, width, height }
    }

    #[test]
    fn blocks_already_apart_stay_where_the_layout_puts_them() {
        let blocks = [block(0, 0, 5, 3), block(20, 0, 5, 3), block(0, 10, 5, 3)];

        assert_eq!(separate(&blocks, Direction::Down, DOWN), [(0, 0), (20, 0), (0, 10)]);
    }

    #[test]
    fn blocks_of_one_rank_move_apart_across_the_flow() {
        let blocks = [block(0, 0, 10, 3), block(6, 0, 5, 3)];

        assert_eq!(separate(&blocks, Direction::Down, DOWN), [(0, 0), (14, 0)]);
    }

    #[test]
    fn a_rank_too_close_to_the_one_before_moves_along_with_every_later_rank() {
        let blocks = [block(0, 0, 5, 3), block(0, 4, 5, 3), block(30, 4, 5, 3), block(0, 12, 5, 3)];

        assert_eq!(separate(&blocks, Direction::Down, DOWN), [(0, 0), (0, 6), (30, 6), (0, 14)]);
    }

    #[test]
    fn a_block_within_a_bigger_blocks_extent_moves_across_it() {
        let group = block(4, 0, 20, 21);
        let beside = block(15, 21, 7, 3);

        assert_eq!(separate(&[group, beside], Direction::Right, Gaps { rank: 6, sibling: 1 }), [(4, 0), (15, 22)]);
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

        let placement = place(&diagram);

        let (group, nodes) = (placement.groups[0], &placement.nodes);
        assert_eq!(group.width, 19 + 6);
        assert!(
            nodes.iter().all(|node| node.x >= group.x + 2 && node.y >= group.y + 2 && node.y + node.height + 2 <= group.y + group.height)
        );
    }
}
