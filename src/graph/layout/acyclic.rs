//! Cycle removal: the links to turn round so every link goes from an earlier rank to a later one.

use super::layered::Link;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Visit {
    Unseen,
    Open,
    Done,
}

/// For each link, whether it closes a cycle: a walk from each block in turn, first named first, along the links in
/// the order they are written, meets it leading back to a block still being walked.
///
/// The source's first block of a cycle stays its first, so a flowchart reads in the order it is written.
pub(super) fn closing(blocks: usize, links: &[Link]) -> Vec<bool> {
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); blocks];
    for (index, link) in links.iter().enumerate() {
        out[link.from].push(index);
    }
    let mut visits = vec![Visit::Unseen; blocks];
    let mut closing = vec![false; links.len()];
    for root in 0..blocks {
        if visits[root] != Visit::Unseen {
            continue;
        }
        visits[root] = Visit::Open;
        let mut walk = vec![(root, 0)];
        while let Some((block, next)) = walk.last_mut() {
            let Some(&link) = out[*block].get(*next) else {
                visits[*block] = Visit::Done;
                walk.pop();
                continue;
            };
            *next += 1;
            let to = links[link].to;
            match visits[to] {
                Visit::Open => closing[link] = true,
                Visit::Unseen => {
                    visits[to] = Visit::Open;
                    walk.push((to, 0));
                }
                Visit::Done => {}
            }
        }
    }
    closing
}

#[cfg(test)]
mod tests {
    use super::*;

    fn links(pairs: &[(usize, usize)]) -> Vec<Link> {
        pairs.iter().map(|&(from, to)| Link { from, to, label: None, from_port: 0, to_port: 0 }).collect()
    }

    #[test]
    fn a_chain_has_no_closing_link() {
        assert_eq!(closing(3, &links(&[(0, 1), (1, 2), (0, 2)])), [false, false, false]);
    }

    #[test]
    fn the_link_back_to_the_first_block_named_closes_the_cycle() {
        assert_eq!(closing(3, &links(&[(0, 1), (1, 2), (2, 0)])), [false, false, true]);
    }

    #[test]
    fn a_link_into_a_block_already_walked_closes_nothing() {
        assert_eq!(closing(3, &links(&[(0, 1), (2, 0), (2, 1)])), [false, false, false]);
    }

    #[test]
    fn two_links_between_the_same_blocks_each_way_close_one_cycle() {
        assert_eq!(closing(2, &links(&[(0, 1), (1, 0)])), [false, true]);
    }
}
