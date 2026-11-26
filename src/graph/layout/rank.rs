//! Layer assignment: a rank for each block, every link going at least one rank on.

use std::collections::BTreeSet;

/// A link once the closing ones are turned round: `from` comes on an earlier rank than `to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Forward {
    pub(super) from: usize,
    pub(super) to: usize,
}

/// Passes over the blocks pulling each towards the side it has more links on; each pass only shortens links, so few
/// are needed.
const PULLS: usize = 8;

/// The earliest rank for each block, then blocks with more links leaving than arriving moved on to shorten them.
pub(super) fn ranks(blocks: usize, links: &[Forward]) -> Vec<usize> {
    let neighbours = Neighbours::new(blocks, links);
    let order = neighbours.in_flow_order();
    let mut ranks = vec![0; blocks];
    for &block in &order {
        ranks[block] = neighbours.before[block].iter().map(|&before| ranks[before] + 1).max().unwrap_or(0);
    }
    for _ in 0..PULLS {
        let pulled = pulled(&ranks, &neighbours, &order);
        if pulled == ranks {
            break;
        }
        ranks = pulled;
    }
    ranks
}

/// What folding needs to know of the room across the flow.
#[derive(Debug, Clone, Copy)]
pub(super) struct Row<'a> {
    /// Cells a row may take.
    pub(super) columns: i32,
    /// Cells each block takes, the gap to the next one included.
    pub(super) breadths: &'a [i32],
    /// Cells a link passing a row takes.
    pub(super) link: i32,
}

/// Ranks no wider than a row allows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Folded {
    pub(super) ranks: Vec<usize>,
    /// Whether each block left the rank it had.
    pub(super) left: Vec<bool>,
}

/// Folds the ranks wider than the row allows: the blocks of a rank that do not fit move to a new rank right after
/// it, and every later rank moves on with them.
///
/// The links from one block to blocks moved on pass together between the blocks that stay, so each such block
/// takes room on the row they leave.
pub(super) fn folded(ranks: &[usize], links: &[Forward], row: Row<'_>) -> Folded {
    let mut ranks = ranks.to_vec();
    let mut left = vec![false; ranks.len()];
    let mut rank = 0;
    while let Some(staying) = staying(&ranks, links, row, rank) {
        if !staying.moved.is_empty() {
            for (block, moved) in ranks.iter_mut().enumerate() {
                *moved += usize::from(*moved > rank || staying.moved.contains(&block));
            }
            for &block in &staying.moved {
                left[block] = true;
            }
        }
        rank += 1;
    }
    Folded { ranks, left }
}

/// The blocks of a rank that leave it.
struct Staying {
    moved: Vec<usize>,
}

/// Which blocks of `rank` move on, `None` past the last rank: the first block always stays, then each next one as
/// long as the row holds it, the links passing the rank and a way through for each block leading to those after it.
fn staying(ranks: &[usize], links: &[Forward], row: Row<'_>, rank: usize) -> Option<Staying> {
    let blocks: Vec<usize> = (0..ranks.len()).filter(|&block| ranks[block] == rank).collect();
    if blocks.is_empty() {
        return None;
    }
    let passing = count(links.iter().filter(|link| ranks[link.from] < rank && rank < ranks[link.to]).count());
    let sources = |blocks: &[usize]| -> usize {
        let arriving = links.iter().filter(|link| blocks.contains(&link.to) && ranks[link.from] < rank).map(|link| link.from);
        arriving.collect::<BTreeSet<usize>>().len()
    };
    let mut used = passing * row.link;
    let mut kept = 0;
    for (index, &block) in blocks.iter().enumerate() {
        let after = count(sources(&blocks[index + 1..])) * row.link;
        if index > 0 && used + row.breadths[block] + after > row.columns {
            break;
        }
        used += row.breadths[block];
        kept = index + 1;
    }
    Some(Staying { moved: blocks[kept..].to_vec() })
}

#[derive(Debug, Clone)]
struct Neighbours {
    before: Vec<Vec<usize>>,
    after: Vec<Vec<usize>>,
}

impl Neighbours {
    fn new(blocks: usize, links: &[Forward]) -> Self {
        let mut neighbours = Self { before: vec![Vec::new(); blocks], after: vec![Vec::new(); blocks] };
        for link in links {
            neighbours.before[link.to].push(link.from);
            neighbours.after[link.from].push(link.to);
        }
        neighbours
    }

    /// Every block after the blocks its links come from.
    fn in_flow_order(&self) -> Vec<usize> {
        let mut waiting: Vec<usize> = self.before.iter().map(Vec::len).collect();
        let mut order: Vec<usize> = (0..waiting.len()).filter(|&block| waiting[block] == 0).collect();
        let mut next = 0;
        while let Some(&block) = order.get(next) {
            next += 1;
            for &after in &self.after[block] {
                waiting[after] -= 1;
                if waiting[after] == 0 {
                    order.push(after);
                }
            }
        }
        order
    }
}

/// A block with more links leaving than arriving goes right before the nearest block it leads to: the links it
/// shortens outnumber those it lengthens.
fn pulled(ranks: &[usize], neighbours: &Neighbours, order: &[usize]) -> Vec<usize> {
    let mut ranks = ranks.to_vec();
    for &block in order.iter().rev() {
        let (before, after) = (&neighbours.before[block], &neighbours.after[block]);
        if after.len() > before.len()
            && let Some(nearest) = after.iter().map(|&after| ranks[after]).min()
        {
            ranks[block] = nearest - 1;
        }
    }
    ranks
}

/// A count of blocks or links in columns' arithmetic: the limits keep it far below `i32::MAX`.
fn count(blocks: usize) -> i32 {
    i32::try_from(blocks).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forward(pairs: &[(usize, usize)]) -> Vec<Forward> {
        pairs.iter().map(|&(from, to)| Forward { from, to }).collect()
    }

    #[test]
    fn a_block_comes_one_rank_after_the_last_block_leading_to_it() {
        assert_eq!(ranks(4, &forward(&[(0, 1), (1, 2), (0, 2), (2, 3)])), [0, 1, 2, 3]);
    }

    #[test]
    fn a_block_only_leading_to_a_late_rank_moves_next_to_it() {
        assert_eq!(ranks(5, &forward(&[(0, 1), (1, 2), (2, 3), (4, 3)])), [0, 1, 2, 3, 2]);
    }

    #[test]
    fn a_block_with_as_many_links_each_side_stays_on_its_earliest_rank() {
        assert_eq!(ranks(5, &forward(&[(0, 1), (1, 2), (2, 3), (0, 4), (4, 3)])), [0, 1, 2, 3, 1]);
    }

    #[test]
    fn a_rank_wider_than_the_row_is_folded_and_later_ranks_move_on() {
        let links = forward(&[(0, 1), (0, 2), (0, 3), (1, 4)]);
        let row = Row { columns: 25, breadths: &[10; 5], link: 3 };

        assert_eq!(
            folded(&[0, 1, 1, 1, 2], &links, row),
            Folded { ranks: vec![0, 1, 1, 2, 3], left: vec![false, false, false, true, false] }
        );
    }

    #[test]
    fn each_block_leading_to_blocks_moved_on_takes_room_on_the_row_they_leave() {
        let one_source = forward(&[(0, 1), (0, 2), (0, 3), (0, 4)]);
        let two_sources = forward(&[(0, 1), (0, 2), (0, 3), (5, 4)]);
        let row = Row { columns: 23, breadths: &[10; 6], link: 3 };

        assert_eq!(folded(&[0, 1, 1, 1, 1], &one_source, row).ranks, [0, 1, 1, 2, 2]);
        assert_eq!(folded(&[0, 1, 1, 1, 1, 0], &two_sources, row).ranks, [0, 1, 2, 2, 3, 0]);
    }

    #[test]
    fn a_rank_that_fits_stays_whole() {
        let row = Row { columns: 30, breadths: &[10; 4], link: 3 };

        assert_eq!(folded(&[0, 1, 1, 1], &forward(&[(0, 1), (0, 2), (0, 3)]), row).left, [false; 4]);
    }

    #[test]
    fn blocks_without_links_all_start_on_the_first_rank() {
        assert_eq!(ranks(3, &[]), [0, 0, 0]);
    }
}
