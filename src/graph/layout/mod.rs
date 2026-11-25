//! The layout: each group laid out in layers as one block of its parent, so its nodes stay inside its frame.

mod acyclic;
mod coordinate;
mod layered;
mod order;
mod rank;

use vt_canvas::{Direction, Rect};

use self::coordinate::Apart;
use self::layered::{Fold, Gaps, Link, Position, Span};
use super::diagram::{Diagram, End};
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
    /// The room kept for each edge's label, when it has one.
    pub(super) labels: Vec<Option<Rect>>,
    /// Whether the edges of one side of a block share a trunk even when the side has room for each.
    pub(super) shares_ways: bool,
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
    labels: Vec<Kept>,
}

/// The room kept for an edge's label, relative to its container's top-left corner.
#[derive(Debug, Clone, Copy)]
struct Kept {
    edge: usize,
    x: i32,
    y: i32,
    width: i32,
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

/// How much room a layout takes across the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Fit {
    /// Blocks and links well apart.
    Loose,
    /// As close as still reads.
    Tight,
    /// Tight, and ranks going down wider than `columns` folded into as many as it takes.
    Folded { columns: i32 },
    /// Folded, and no room kept for the links passing a rank nor for labels: the last resort, as edges then cross.
    Packed { columns: i32 },
}

impl Fit {
    /// The fit inside a group's frame and padding.
    fn inside_group(self) -> Self {
        match self {
            Self::Folded { columns } => Self::Folded { columns: columns - 2 * GROUP_PADDING },
            Self::Packed { columns } => Self::Packed { columns: columns - 2 * GROUP_PADDING },
            fit => fit,
        }
    }
}

/// Lays `diagram` out in cells.
pub(super) fn place(diagram: &Diagram, fit: Fit) -> Placement {
    let root = arrange(diagram, None, fit);
    let margin = i32::from(MARGIN);
    let mut placement = Placement {
        nodes: vec![Rect::new(0, 0, 0, 0); diagram.nodes.len()],
        groups: vec![Rect::new(0, 0, 0, 0); diagram.groups.len()],
        labels: vec![None; diagram.edges.len()],
        shares_ways: matches!(fit, Fit::Folded { .. } | Fit::Packed { .. }),
        size: Size { width: to_cells(root.width + 2 * margin), height: to_cells(root.height + 2 * margin) },
    };
    for (item, rect) in flatten(&root, margin, margin) {
        match item {
            Item::Node(index) => placement.nodes[index] = rect,
            Item::Group(index) => placement.groups[index] = rect,
        }
    }
    for (edge, rect) in kept(&root, margin, margin) {
        placement.labels[edge] = Some(rect);
    }
    placement
}

/// The children of `container` in layers going the way it flows, each child group arranged first.
fn arrange(diagram: &Diagram, container: Option<usize>, fit: Fit) -> Arrangement {
    let items = items(diagram, container);
    let contents: Vec<Arrangement> = items
        .iter()
        .map(|item| match item {
            Item::Node(_) => Arrangement::default(),
            Item::Group(index) => arrange(diagram, Some(*index), fit.inside_group()),
        })
        .collect();
    let flow = container.map_or(diagram.flow, |group| diagram.groups[group].flow);
    let vertical = matches!(flow, Direction::Down | Direction::Up);
    let sizes: Vec<(i32, i32)> = items.iter().zip(&contents).map(|(item, content)| size(diagram, *item, content)).collect();
    let spans: Vec<Span> = sizes.iter().map(|&(width, height)| span(width, height, vertical)).collect();
    let blocks = Blocks { items: &items, contents: &contents, sizes: &sizes, vertical };
    let links = links(diagram, container, &blocks);
    let joined: Vec<Link> = links.iter().map(|&(link, _)| link).collect();
    let fold = match fit {
        Fit::Folded { columns } if vertical => Some(Fold { columns, links_take_room: true }),
        Fit::Packed { columns } if vertical => Some(Fold { columns, links_take_room: false }),
        _ => None,
    };
    let placed = layered::placed(&spans, &joined, gaps(vertical, fit), fold);
    let length = placed.blocks.iter().zip(&spans).map(|(position, span)| position.along + span.length).max().unwrap_or(0);
    let corner = |position: Position, span: Span| {
        let along = if matches!(flow, Direction::Up | Direction::Left) { length - position.along - span.length } else { position.along };
        if vertical { (position.across, along) } else { (along, position.across) }
    };
    let labels: Vec<Kept> = links
        .iter()
        .zip(&placed.labels)
        .filter_map(|(&(link, edge), position)| {
            let (span, edge) = (link.label?, edge?);
            let (x, y) = corner((*position)?, span);
            Some(Kept { edge, x, y, width: if vertical { span.breadth } else { span.length } })
        })
        .collect();
    let children: Vec<Child> = items
        .into_iter()
        .zip(contents)
        .zip(sizes.iter().zip(placed.blocks.iter().zip(&spans)))
        .map(|((item, content), (&(width, height), (&position, &span)))| {
            let (x, y) = corner(position, span);
            Child { item, x, y, width, height, content }
        })
        .collect();
    let ends = children.iter().map(|child| (child.x + child.width, child.y + child.height));
    let label_ends = labels.iter().map(|label| (label.x + label.width, label.y + 1));
    let (width, height) = ends.chain(label_ends).fold((0, 0), |(width, height), (right, bottom)| (width.max(right), height.max(bottom)));
    Arrangement { width, height, children, labels }
}

/// The nodes and groups `container` holds directly, in the order the source first names them or a node of theirs.
fn items(diagram: &Diagram, container: Option<usize>) -> Vec<Item> {
    let nodes = (0..diagram.nodes.len()).filter(|&index| diagram.nodes[index].group == container).map(Item::Node);
    let groups = (0..diagram.groups.len()).filter(|&index| diagram.groups[index].parent == container).map(Item::Group);
    let mut items: Vec<Item> = nodes.chain(groups).collect();
    let first_named = |item: Item| match item {
        Item::Node(index) => diagram.nodes[index].order,
        Item::Group(group) => {
            diagram.nodes.iter().filter(|node| holds(diagram, group, node.group)).map(|node| node.order).min().unwrap_or(usize::MAX)
        }
    };
    items.sort_by_key(|&item| first_named(item));
    items
}

/// Whether `group` is `inner` or holds it, at any depth.
fn holds(diagram: &Diagram, group: usize, inner: Option<usize>) -> bool {
    std::iter::successors(inner, |&index| diagram.groups[index].parent).take(diagram.groups.len() + 1).any(|index| index == group)
}

fn size(diagram: &Diagram, item: Item, content: &Arrangement) -> (i32, i32) {
    match item {
        Item::Node(index) => {
            let size = shape::size(diagram.nodes[index].shape, &diagram.nodes[index].label);
            (i32::from(size.width), i32::from(size.height))
        }
        Item::Group(index) => {
            let title = i32::from(diagram.groups[index].title.width) + TITLE_FRAME;
            ((content.width + 2 * GROUP_PADDING).max(title), content.height + 2 * GROUP_PADDING)
        }
    }
}

fn span(width: i32, height: i32, vertical: bool) -> Span {
    if vertical { Span { length: height, breadth: width } } else { Span { length: width, breadth: height } }
}

/// Room for a bend and an arrow between ranks, and a cell for a bend each side of a label; going down a loose fit
/// keeps blocks four columns apart and links two, a tight one two and one.
fn gaps(vertical: bool, fit: Fit) -> Gaps {
    match (vertical, fit) {
        (true, Fit::Loose) => Gaps { rank: 3, around_label: 1, apart: Apart { blocks: 4, links: 2 } },
        (true, _) => Gaps { rank: 3, around_label: 1, apart: Apart { blocks: 2, links: 1 } },
        (false, _) => Gaps { rank: 6, around_label: 2, apart: Apart { blocks: 1, links: 1 } },
    }
}

/// The room a label takes between two ranks: a row going down; going sideways its columns and one more, so a
/// line cell shows each side of it.
fn label_span(width: u16, vertical: bool) -> Span {
    if vertical { Span { length: 1, breadth: i32::from(width) } } else { Span { length: i32::from(width) + 1, breadth: 1 } }
}

/// The children of a container before they are placed.
#[derive(Debug, Clone, Copy)]
struct Blocks<'a> {
    items: &'a [Item],
    contents: &'a [Arrangement],
    sizes: &'a [(i32, i32)],
    vertical: bool,
}

impl Blocks<'_> {
    /// The block that is `end` or holds it, and the cells across the flow from the block's first cell to the
    /// middle of `end`: a link to a node inside a group is straight when it is in line with the node.
    fn joined(&self, diagram: &Diagram, container: Option<usize>, end: End) -> Option<(usize, i32)> {
        let held = item_holding(diagram, container, end)?;
        let block = self.items.iter().position(|item| *item == held)?;
        let item = match end {
            End::Node(index) => Item::Node(index),
            End::Group(index) => Item::Group(index),
        };
        let (x, y, width, height) = if item == held {
            (0, 0, self.sizes[block].0, self.sizes[block].1)
        } else {
            corner(&self.contents[block], item, GROUP_PADDING, GROUP_PADDING)?
        };
        Some((block, if self.vertical { x + (width - 1) / 2 } else { y + (height - 1) / 2 }))
    }
}

/// Where `item` lies inside `arrangement`, itself at (`x`, `y`), and its size.
fn corner(arrangement: &Arrangement, item: Item, x: i32, y: i32) -> Option<(i32, i32, i32, i32)> {
    arrangement.children.iter().find_map(|child| {
        let (left, top) = (x + child.x, y + child.y);
        if child.item == item {
            Some((left, top, child.width, child.height))
        } else {
            corner(&child.content, item, left + GROUP_PADDING, top + GROUP_PADDING)
        }
    })
}

/// The edges and invisible links joining two of the blocks, whatever their ends' depth inside them, each edge with
/// its place among the edges.
fn links(diagram: &Diagram, container: Option<usize>, blocks: &Blocks<'_>) -> Vec<(Link, Option<usize>)> {
    let edges = diagram.edges.iter().enumerate().map(|(index, edge)| (edge.from, edge.to, edge.label.as_ref(), Some(index)));
    let invisible = diagram.invisible.iter().map(|link| (link.from, link.to, None, None));
    edges
        .chain(invisible)
        .filter_map(|(from, to, label, edge)| {
            let ((from, from_port), (to, to_port)) = (blocks.joined(diagram, container, from)?, blocks.joined(diagram, container, to)?);
            let label = label.map(|text| label_span(text.width, blocks.vertical));
            Some((Link { from, to, label, from_port, to_port }, edge))
        })
        .filter(|(link, _)| link.from != link.to)
        .collect()
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

/// The room kept for each edge's label, in grid cells.
fn kept(arrangement: &Arrangement, x: i32, y: i32) -> Vec<(usize, Rect)> {
    let own = arrangement
        .labels
        .iter()
        .map(|label| (label.edge, Rect::new(to_cells(x + label.x), to_cells(y + label.y), to_cells(label.width), 1)));
    let inner =
        arrangement.children.iter().flat_map(|child| kept(&child.content, x + child.x + GROUP_PADDING, y + child.y + GROUP_PADDING));
    own.chain(inner).collect()
}

fn to_cells(value: i32) -> u16 {
    u16::try_from(value.max(0)).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flowchart;

    fn placed(source: &str, fit: Fit) -> (Diagram, Placement) {
        let diagram = flowchart::read(source).unwrap();
        let placement = place(&diagram, fit);
        (diagram, placement)
    }

    fn node(diagram: &Diagram, placement: &Placement, id: &str) -> Rect {
        placement.nodes[diagram.nodes.iter().position(|node| node.id == id).unwrap()]
    }

    #[test]
    fn a_chain_going_down_is_one_column_three_rows_apart() {
        let (diagram, placement) = placed("flowchart TB\n  A[Start] --> B[A longer step] --> C[End]", Fit::Loose);

        let rects = ["A", "B", "C"].map(|id| node(&diagram, &placement, id));
        assert_eq!(rects, [Rect::new(7, 3, 9, 3), Rect::new(3, 9, 17, 3), Rect::new(8, 15, 7, 3)]);
    }

    #[test]
    fn a_flow_going_up_or_left_puts_the_first_rank_last() {
        let (up, placement_up) = placed("flowchart BT\n  A --> B", Fit::Loose);
        let (left, placement_left) = placed("flowchart RL\n  A --> B", Fit::Loose);

        assert!(node(&up, &placement_up, "A").y > node(&up, &placement_up, "B").y);
        assert!(node(&left, &placement_left, "A").x > node(&left, &placement_left, "B").x);
    }

    #[test]
    fn a_group_is_one_block_whose_frame_holds_its_nodes_and_flows_its_own_way() {
        let source = "flowchart TB\n  A --> B\n  subgraph Side [Sideways]\n    direction LR\n    B --> C\n  end\n  C --> D";

        let (diagram, placement) = placed(source, Fit::Loose);

        let (frame, b, c) = (placement.groups[0], node(&diagram, &placement, "B"), node(&diagram, &placement, "C"));
        assert_eq!((b.y, c.x > b.x + b.width), (c.y, true));
        assert_eq!((frame.x, frame.y), (b.x - 2, b.y - 2));
        assert_eq!(frame.x + frame.width, c.x + c.width + 2);
        assert!(node(&diagram, &placement, "D").y >= frame.y + frame.height + 3);
    }

    #[test]
    fn a_link_to_a_node_inside_a_group_is_in_line_with_the_node() {
        let source = "flowchart TB\n  A --> C\n  subgraph Wide\n    direction LR\n    B[A first wide node] --> C\n  end";

        let (diagram, placement) = placed(source, Fit::Loose);

        let middle = |rect: Rect| rect.x + (rect.width - 1) / 2;
        assert_eq!(middle(node(&diagram, &placement, "A")), middle(node(&diagram, &placement, "C")));
    }

    #[test]
    fn room_is_kept_for_a_label_above_the_block_its_edge_leads_to() {
        let (diagram, placement) = placed("flowchart TB\n  A -->|yes| B\n  A -->|no| C", Fit::Loose);

        let b = node(&diagram, &placement, "B");
        assert_eq!(placement.labels[0], Some(Rect::new(b.x + 1, b.y - 2, 3, 1)));
        assert_eq!(placement.labels.len(), 2);
    }

    #[test]
    fn a_label_wider_than_the_blocks_widens_the_drawing() {
        let (_, placement) = placed("flowchart TB\n  A -->|a label far wider than its nodes| B", Fit::Loose);

        let label = placement.labels[0].unwrap();
        assert_eq!(label.width, 32);
        assert!(label.x + label.width + MARGIN <= placement.size.width, "{placement:?}");
    }

    #[test]
    fn an_invisible_link_ranks_its_nodes_without_an_edge() {
        let (diagram, placement) = placed("flowchart LR\n  A ~~~ B", Fit::Loose);

        assert!(node(&diagram, &placement, "B").x >= node(&diagram, &placement, "A").x + 5 + 6);
    }

    #[test]
    fn a_tight_fit_stands_blocks_closer_and_a_folded_one_asks_edges_to_share() {
        let source = "flowchart TB\n  A --> B & C";

        let gap = |fit: Fit| {
            let (diagram, placement) = placed(source, fit);
            let (b, c) = (node(&diagram, &placement, "B"), node(&diagram, &placement, "C"));
            (c.x - b.x - b.width, placement.shares_ways)
        };

        assert_eq!([Fit::Loose, Fit::Tight, Fit::Folded { columns: 80 }].map(gap), [(4, false), (2, false), (2, true)]);
    }

    #[test]
    fn a_rank_wider_than_the_columns_is_folded_inside_a_group_too() {
        let source = "flowchart TB\n  subgraph G\n    A --> B[First option] & C[Second option] & D[Third option]\n  end";

        let (diagram, placement) = placed(source, Fit::Folded { columns: 44 });
        let (_, packed) = placed(source, Fit::Packed { columns: 44 });

        let rows: Vec<u16> = ["B", "C", "D"].map(|id| node(&diagram, &placement, id).y).to_vec();
        assert!(rows[0] == rows[1] && rows[2] > rows[1], "{rows:?}");
        assert!(placement.groups[0].width <= 44 && packed.groups[0].width <= placement.groups[0].width);
    }
}
