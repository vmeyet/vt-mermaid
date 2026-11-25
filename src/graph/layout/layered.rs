//! A layered drawing of blocks joined by links, in cells along and across the flow whichever way it goes.

use std::collections::HashMap;

use super::acyclic;
use super::coordinate::{self, Apart, Segment, Width};
use super::order::{self, Layered};
use super::rank::{self, Folded, Forward, Row};

/// Cells along the flow and across it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct Span {
    pub(super) length: i32,
    pub(super) breadth: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Link {
    pub(super) from: usize,
    pub(super) to: usize,
    /// The room its label takes between two ranks.
    pub(super) label: Option<Span>,
    /// Cells across the flow from each block's first cell to where the link joins it.
    pub(super) from_port: i32,
    pub(super) to_port: i32,
}

/// Cells kept free between parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Gaps {
    /// Between two ranks.
    pub(super) rank: i32,
    /// Between a rank and a label in the gap after it, and between the label and the next rank.
    pub(super) around_label: i32,
    pub(super) apart: Apart,
}

/// A block's first cell along the flow and across it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Position {
    pub(super) along: i32,
    pub(super) across: i32,
}

/// The weight of a segment that has to be straight: between two dummies, and after a label, which stands on it.
const STRAIGHT: i64 = 16;

/// A place on a layer: a block, or a link passing the layer on its way, with its label when it has it there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Vertex {
    span: Span,
    on_link: bool,
}

/// How ranks wider than the screen are folded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Fold {
    /// Cells across the flow a rank may take.
    pub(super) columns: i32,
    /// Whether a link passing a rank and a label keep room of their own: without, blocks stand closer and the
    /// links find their way round them.
    pub(super) links_take_room: bool,
}

/// Where the blocks go, and the room kept for each link's label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Placed {
    pub(super) blocks: Vec<Position>,
    /// In the links' order, `None` for a link without a label.
    pub(super) labels: Vec<Option<Position>>,
}

/// Places every block: links closing a cycle turned round, a rank for each block, ranks wider than `columns`
/// folded, each rank ordered to uncross the links, then cells across and along the flow.
///
/// A link's label takes a layer of its own between two ranks, so it has room whatever stands beside it.
pub(super) fn placed(blocks: &[Span], links: &[Link], gaps: Gaps, fold: Option<Fold>) -> Placed {
    let closing = acyclic::closing(blocks.len(), links);
    let forward: Vec<Forward> = links
        .iter()
        .zip(&closing)
        .map(|(link, &closing)| if closing { Forward { from: link.to, to: link.from } } else { Forward { from: link.from, to: link.to } })
        .collect();
    let ranks = rank::ranks(blocks.len(), &forward);
    let folded = match fold {
        Some(Fold { columns, links_take_room }) => {
            let breadths = footprints(blocks, links, &forward, gaps.apart);
            let link = if links_take_room { 1 + gaps.apart.links } else { 0 };
            rank::folded(&ranks, &forward, Row { columns: columns + gaps.apart.blocks, breadths: &breadths, link })
        }
        None => Folded { left: vec![false; blocks.len()], ranks },
    };
    let packed = fold.is_some_and(|fold| !fold.links_take_room);
    let drawing = Drawing::new(blocks, links, &closing, &Ranked { ranks: &folded.ranks, folded: &folded.left, packed });
    let layers = order::ordered(&drawing.layered);
    let widths: Vec<Width> =
        drawing.vertices.iter().map(|vertex| Width { breadth: vertex.span.breadth, on_link: vertex.on_link }).collect();
    let across = coordinate::across(&layers, &widths, &drawing.segments, gaps.apart);
    let along = drawing.along(gaps);
    let position = |vertex: usize| Position { along: along[drawing.layer_of[vertex]], across: across[vertex] };
    Placed { blocks: (0..blocks.len()).map(position).collect(), labels: drawing.labels.iter().map(|label| label.map(position)).collect() }
}

/// The cells each block takes on a row with the gap to the next one: its own or those of the labels of the links
/// arriving, which stand side by side before it.
fn footprints(blocks: &[Span], links: &[Link], forward: &[Forward], apart: Apart) -> Vec<i32> {
    let mut labels = vec![0; blocks.len()];
    for (link, forward) in links.iter().zip(forward) {
        labels[forward.to] += link.label.map_or(0, |label| label.breadth + apart.links);
    }
    blocks.iter().zip(labels).map(|(block, labels)| block.breadth.max(labels - apart.links) + apart.blocks).collect()
}

/// Where the ranking put each block.
#[derive(Debug, Clone, Copy)]
struct Ranked<'a> {
    ranks: &'a [usize],
    /// Whether the block left a rank too wide: its links then turn to reach it, so it need not stand in line.
    folded: &'a [bool],
    /// Whether only links from one rank to the next count, and no label has a layer.
    packed: bool,
}

/// The blocks and the dummies of their links on layers: a layer per rank, and one more after each rank a label
/// follows.
#[derive(Debug, Clone)]
struct Drawing {
    vertices: Vec<Vertex>,
    layer_of: Vec<usize>,
    /// Whether each layer holds labels instead of blocks.
    of_labels: Vec<bool>,
    layered: Layered,
    segments: Vec<Segment>,
    /// The dummy as big as each link's label.
    labels: Vec<Option<usize>>,
    /// The dummy the links from a block to blocks that left a rank too wide share on a layer, by block and layer.
    shared: HashMap<(usize, usize), usize>,
}

/// A link as it is drawn, from the earlier rank to the later one.
#[derive(Debug, Clone, Copy)]
struct Path {
    above: usize,
    below: usize,
    above_port: i32,
    below_port: i32,
    /// It closes a cycle, so it is drawn against the flow and gives way to the others.
    closing: bool,
    label: Option<(Span, usize)>,
    /// Its last block left a rank too wide.
    folded: bool,
    /// It goes from one rank to the next.
    short: bool,
}

impl Drawing {
    fn new(blocks: &[Span], links: &[Link], closing: &[bool], ranked: &Ranked<'_>) -> Self {
        let ranks = ranked.ranks;
        let ends = |link: &Link, closing: bool| if closing { (link.to, link.from) } else { (link.from, link.to) };
        let label_gaps: Vec<Option<usize>> = links
            .iter()
            .zip(closing)
            .map(|(link, &closing)| {
                let (above, below) = ends(link, closing);
                link.label.filter(|_| !ranked.packed).map(|_| label_gap(ranks[above], ranks[below]))
            })
            .collect();
        let rank_count = ranks.iter().max().map_or(0, |last| last + 1);
        let mut layer_of_rank = Vec::with_capacity(rank_count);
        let mut of_labels = Vec::new();
        for rank in 0..rank_count {
            layer_of_rank.push(of_labels.len());
            of_labels.push(false);
            if label_gaps.contains(&Some(rank)) {
                of_labels.push(true);
            }
        }
        let mut drawing = Self {
            vertices: blocks.iter().map(|&span| Vertex { span, on_link: false }).collect(),
            layer_of: ranks.iter().map(|&rank| layer_of_rank[rank]).collect(),
            layered: Layered {
                layers: vec![Vec::new(); of_labels.len()],
                up: vec![Vec::new(); blocks.len()],
                down: vec![Vec::new(); blocks.len()],
            },
            of_labels,
            segments: Vec::new(),
            labels: Vec::with_capacity(links.len()),
            shared: HashMap::new(),
        };
        for (block, &layer) in drawing.layer_of.clone().iter().enumerate() {
            drawing.layered.layers[layer].push(block);
        }
        for ((link, &closing), gap) in links.iter().zip(closing).zip(label_gaps) {
            let (above, below) = ends(link, closing);
            let (above_port, below_port) = if closing { (link.to_port, link.from_port) } else { (link.from_port, link.to_port) };
            let label = link.label.zip(gap.map(|gap| layer_of_rank[gap] + 1));
            if ranked.packed && ranks[below] > ranks[above] + 1 {
                drawing.labels.push(None);
                continue;
            }
            let short = ranks[below] == ranks[above] + 1;
            drawing.add(Path { above, below, above_port, below_port, closing, label, folded: ranked.folded[below], short });
        }
        drawing
    }

    /// Joins the path's ends through a dummy on every layer between them, the one on its label's layer as big as
    /// the label and in line with what comes after it, so the label stands on the last straight piece.
    fn add(&mut self, path: Path) {
        let (mut above, mut above_port) = (path.above, path.above_port);
        let mut label = None;
        let mut after_label = false;
        for layer in self.layer_of[path.above] + 1..self.layer_of[path.below] {
            let labelled = path.label.filter(|&(_, label_layer)| label_layer == layer).map(|(span, _)| span);
            let span = labelled.unwrap_or(Span { length: 0, breadth: 1 });
            let port = (span.breadth - 1) / 2;
            let shares = path.folded && path.label.is_none() && !path.closing;
            if let Some(&dummy) = self.shared.get(&(path.above, layer)).filter(|_| shares) {
                (above, above_port) = (dummy, port);
                continue;
            }
            let dummy = self.vertices.len();
            label = label.or(labelled.map(|_| dummy));
            self.vertices.push(Vertex { span, on_link: true });
            self.layer_of.push(layer);
            self.layered.layers[layer].push(dummy);
            self.layered.up.push(Vec::new());
            self.layered.down.push(Vec::new());
            if shares {
                self.shared.insert((path.above, layer), dummy);
            }
            self.join(Segment {
                above,
                below: dummy,
                above_port,
                below_port: port,
                weight: if after_label { STRAIGHT } else { self.weight(above, dummy, &path) },
            });
            (above, above_port, after_label) = (dummy, port, labelled.is_some());
        }
        let weight = if after_label && !path.folded { STRAIGHT } else { self.weight(above, path.below, &path) };
        self.join(Segment { above, below: path.below, above_port, below_port: path.below_port, weight });
        self.labels.push(label);
    }

    /// How much a segment's straightness matters: most between two dummies, so a long link is one line; then from
    /// one rank to the next, so a chain of blocks is one line whatever long links leave it; least at the ends of a
    /// long link and on a link closing a cycle, which bend there; not at all into a block that left a rank too
    /// wide, which stays where it takes the least room.
    fn weight(&self, above: usize, below: usize, path: &Path) -> i64 {
        match (self.vertices[above].on_link, self.vertices[below].on_link) {
            (_, false) if path.folded => 0,
            (true, true) => STRAIGHT,
            _ if path.short && !path.closing => 8,
            _ => 1,
        }
    }

    fn join(&mut self, segment: Segment) {
        self.layered.down[segment.above].push(segment.below);
        self.layered.up[segment.below].push(segment.above);
        self.segments.push(segment);
    }

    /// The first cell along the flow of each layer.
    fn along(&self, gaps: Gaps) -> Vec<i32> {
        let mut lengths = vec![0; self.of_labels.len()];
        for (vertex, &layer) in self.vertices.iter().zip(&self.layer_of) {
            lengths[layer] = vertex.span.length.max(lengths[layer]);
        }
        let mut starts = Vec::with_capacity(lengths.len());
        let mut next = 0;
        for (layer, &length) in lengths.iter().enumerate() {
            let label_follows = self.of_labels.get(layer + 1).copied().unwrap_or(false);
            let start = if self.of_labels[layer] { next + gaps.around_label } else { next };
            starts.push(start);
            next = if self.of_labels[layer] {
                (start + length + gaps.around_label).max(next + gaps.rank)
            } else if label_follows {
                start + length
            } else {
                start + length + gaps.rank
            };
        }
        starts
    }
}

/// The gap a link from rank `from` to rank `to` has its label in, counted by the rank before it: the middle one.
fn label_gap(from: usize, to: usize) -> usize {
    from + (to - from - 1) / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOWN: Gaps = Gaps { rank: 3, around_label: 1, apart: Apart { blocks: 4, links: 2 } };
    const BLOCK: Span = Span { length: 3, breadth: 7 };

    fn link(from: usize, to: usize) -> Link {
        Link { from, to, label: None, from_port: 3, to_port: 3 }
    }

    #[test]
    fn a_chain_goes_rank_after_rank_on_one_line() {
        let placed = placed(&[BLOCK; 3], &[link(0, 1), link(1, 2)], DOWN, None);

        assert_eq!(placed.blocks.iter().map(|position| (position.along, position.across)).collect::<Vec<_>>(), [(0, 0), (6, 0), (12, 0)]);
    }

    #[test]
    fn a_link_closing_a_cycle_leaves_its_blocks_in_the_order_written() {
        let placed = placed(&[BLOCK; 3], &[link(0, 1), link(1, 2), link(2, 0)], DOWN, None);

        assert_eq!(placed.blocks.iter().map(|position| position.along).collect::<Vec<_>>(), [0, 6, 12]);
    }

    #[test]
    fn a_label_wider_than_the_gap_moves_the_next_rank_on() {
        let across = Gaps { rank: 6, around_label: 2, apart: Apart { blocks: 1, links: 1 } };
        let labelled = Link { label: Some(Span { length: 9, breadth: 1 }), ..link(0, 1) };

        let placed = placed(&[BLOCK; 2], &[labelled], across, None);

        assert_eq!(placed.blocks[1].along, 3 + 2 + 9 + 2);
    }

    #[test]
    fn a_label_as_long_as_the_gap_allows_moves_nothing() {
        let labelled = Link { label: Some(Span { length: 1, breadth: 5 }), ..link(0, 1) };

        let placed = placed(&[BLOCK; 2], &[labelled], DOWN, None);

        assert_eq!(placed.blocks[1].along, 6);
    }

    #[test]
    fn a_label_is_placed_in_line_with_the_block_it_leads_to() {
        let label = Some(Span { length: 1, breadth: 5 });

        let placed = placed(&[BLOCK; 3], &[Link { label, ..link(0, 1) }, link(0, 2)], DOWN, None);

        assert_eq!(placed.labels, [Some(Position { along: 4, across: placed.blocks[1].across + 1 }), None]);
    }

    #[test]
    fn a_fold_keeps_room_on_a_rank_for_the_links_passing_it_unless_it_packs() {
        let links = [link(0, 1), link(0, 2), link(0, 3)];
        let fold = |links_take_room: bool| Some(Fold { columns: 20, links_take_room });

        let folded = placed(&[BLOCK; 4], &links, DOWN, fold(true));
        let packed = placed(&[BLOCK; 4], &links, DOWN, fold(false));

        let ranks = |placed: &Placed| placed.blocks.iter().map(|position| position.along).collect::<Vec<_>>();
        assert_eq!((ranks(&folded), ranks(&packed)), (vec![0, 6, 12, 12], vec![0, 6, 6, 12]));
    }

    #[test]
    fn labels_side_by_side_keep_their_blocks_apart() {
        let label = Some(Span { length: 1, breadth: 20 });

        let placed = placed(&[BLOCK; 3], &[Link { label, ..link(0, 1) }, Link { label, ..link(0, 2) }], DOWN, None);

        assert_eq!(placed.blocks[2].across - placed.blocks[1].across, 20 + 2);
    }
}
