//! Coordinate assignment: where each vertex goes across the flow, links as straight as the order allows.

/// A vertex as the placement sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Width {
    /// Cells it covers across the flow.
    pub(super) breadth: i32,
    /// A dummy of a link, not a block.
    pub(super) on_link: bool,
}

/// A link's piece between two neighbouring layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Segment {
    pub(super) above: usize,
    pub(super) below: usize,
    /// How much it matters that the piece is straight.
    pub(super) weight: i64,
    /// Cells from each vertex's first cell to where the piece joins it.
    pub(super) above_port: i32,
    pub(super) below_port: i32,
}

/// Cells kept between neighbours on a layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Apart {
    pub(super) blocks: i32,
    /// Between a link and a block or another link.
    pub(super) links: i32,
}

/// Sweeps down then up tried; the placement that costs least is kept.
const ROUNDS: usize = 4;
/// Passes pulling every layer towards both its neighbours at once, kept while they cost less.
const SETTLES: usize = 8;
/// A vertex without a neighbour holds its place with this weight; a segment weighs [`SCALE`] times its own.
const HELD: i64 = 1;
const SCALE: i64 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Towards {
    Up,
    Down,
    Both,
}

/// The first cell across the flow of each vertex, the leftmost at 0.
///
/// A layer is placed whole: each vertex asks for the place that makes its segments straight, the middle one when
/// it has several, and the layer gets the places closest to what its vertices ask that keep their order and their
/// distance, so vertices asking for one place end up centred on it.
pub(super) fn across(layers: &[Vec<usize>], widths: &[Width], segments: &[Segment], apart: Apart) -> Vec<i32> {
    let rows = Rows::new(layers, widths, segments, apart);
    let mut places = rows.offsets.clone();
    let mut best = places.clone();
    let mut lowest = rows.cost(&places);
    for towards in [Towards::Up, Towards::Down].into_iter().cycle().take(2 * ROUNDS) {
        places = rows.swept(places, towards);
        let cost = rows.cost(&places);
        if cost < lowest {
            (lowest, best) = (cost, places.clone());
        }
    }
    for _ in 0..SETTLES {
        let settled = rows.swept(best.clone(), Towards::Both);
        let cost = rows.cost(&settled);
        if cost >= lowest {
            break;
        }
        (lowest, best) = (cost, settled);
    }
    let left = best.iter().copied().min().unwrap_or(0);
    best.into_iter().map(|place| place - left).collect()
}

/// A segment seen from one of its ends: the start that end's vertex needs for the segment to be straight is the
/// neighbour's start plus `shift`.
#[derive(Debug, Clone, Copy)]
struct Pull {
    neighbour: usize,
    shift: i32,
    weight: i64,
}

struct Rows<'a> {
    layers: &'a [Vec<usize>],
    up: Vec<Vec<Pull>>,
    down: Vec<Vec<Pull>>,
    /// Each vertex's first cell when its layer is packed from 0.
    offsets: Vec<i32>,
}

impl<'a> Rows<'a> {
    fn new(layers: &'a [Vec<usize>], widths: &[Width], segments: &[Segment], apart: Apart) -> Self {
        let mut offsets = vec![0; widths.len()];
        for layer in layers {
            for pair in layer.windows(2) {
                let (before, vertex) = (pair[0], pair[1]);
                let gap = if widths[before].on_link || widths[vertex].on_link { apart.links } else { apart.blocks };
                offsets[vertex] = offsets[before] + widths[before].breadth + gap;
            }
        }
        let (mut up, mut down) = (vec![Vec::new(); widths.len()], vec![Vec::new(); widths.len()]);
        for segment in segments {
            let shift = segment.above_port - segment.below_port;
            up[segment.below].push(Pull { neighbour: segment.above, shift, weight: segment.weight });
            down[segment.above].push(Pull { neighbour: segment.below, shift: -shift, weight: segment.weight });
        }
        Self { layers, up, down, offsets }
    }

    /// What a placement costs: the cells every segment covers across the flow, weighted, and for each vertex with
    /// several segments on one side, how far it stands from their middle, so a block stays centred on the blocks it
    /// leads to rather than straight above one of them.
    fn cost(&self, places: &[i32]) -> i64 {
        let bent = |vertex: usize| {
            self.down[vertex]
                .iter()
                .map(|pull| pull.weight * i64::from((places[pull.neighbour] + pull.shift - places[vertex]).abs()))
                .sum::<i64>()
        };
        let off_centre = |vertex: usize, pulls: &[Pull]| {
            let weight: i64 = pulls.iter().map(|pull| pull.weight).sum();
            let centre = middle(pulls, places).filter(|_| pulls.len() > 1 && weight > 0);
            centre.map_or(0, |centre| weight * i64::from((places[vertex] - centre).abs()))
        };
        (0..places.len()).map(|vertex| bent(vertex) + off_centre(vertex, &self.up[vertex]) + off_centre(vertex, &self.down[vertex])).sum()
    }

    fn swept(&self, places: Vec<i32>, towards: Towards) -> Vec<i32> {
        let mut places = places;
        let order: Vec<usize> = match towards {
            Towards::Up => (1..self.layers.len()).collect(),
            Towards::Down => (0..self.layers.len().saturating_sub(1)).rev().collect(),
            Towards::Both => (0..self.layers.len()).collect(),
        };
        for index in order {
            let asked: Vec<Asked> = self.layers[index].iter().map(|&vertex| self.asked(vertex, &places, towards)).collect();
            for (&vertex, start) in self.layers[index].iter().zip(closest_in_order(&asked)) {
                places[vertex] = start + self.offsets[vertex];
            }
        }
        places
    }

    /// Where `vertex` would start, less its offset, for its middle segment to be straight.
    fn asked(&self, vertex: usize, places: &[i32], towards: Towards) -> Asked {
        let pulls = match towards {
            Towards::Up => self.up[vertex].clone(),
            Towards::Down => self.down[vertex].clone(),
            Towards::Both => self.up[vertex].iter().chain(&self.down[vertex]).copied().collect(),
        };
        let pulls: Vec<Pull> = pulls.into_iter().filter(|pull| pull.weight > 0).collect();
        match middle(&pulls, places) {
            Some(start) => {
                Asked { start: i64::from(start - self.offsets[vertex]), weight: SCALE * pulls.iter().map(|pull| pull.weight).sum::<i64>() }
            }
            None => Asked { start: i64::from(places[vertex] - self.offsets[vertex]), weight: HELD },
        }
    }
}

/// The start that leaves half the weight of `pulls` on each side.
fn middle(pulls: &[Pull], places: &[i32]) -> Option<i32> {
    let mut starts: Vec<(i32, i64)> = pulls.iter().map(|pull| (places[pull.neighbour] + pull.shift, pull.weight)).collect();
    starts.sort_unstable();
    median(&starts)
}

/// The value half the weight lies on each side of; between two values when the weight splits evenly.
fn median(sorted: &[(i32, i64)]) -> Option<i32> {
    let total: i64 = sorted.iter().map(|&(_, weight)| weight).sum();
    let reached = |twice: i64| {
        let mut sum = 0;
        sorted.iter().find(|&&(_, weight)| {
            sum += weight;
            2 * sum > twice
        })
    };
    let (low, high) = (reached(total - 1)?.0, reached(total)?.0);
    Some((low + high).div_euclid(2))
}

/// A start a vertex asks for, and how much it weighs against its neighbours'.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Asked {
    start: i64,
    weight: i64,
}

/// Starts that never fall from one vertex to the next, as close to those `asked` as their weights allow: vertices
/// asking for starts out of order share the weighted average of theirs.
fn closest_in_order(asked: &[Asked]) -> Vec<i32> {
    struct Pool {
        sum: i64,
        weight: i64,
        count: usize,
    }
    let mut pools: Vec<Pool> = Vec::with_capacity(asked.len());
    for ask in asked {
        let mut pool = Pool { sum: ask.start * ask.weight, weight: ask.weight, count: 1 };
        while let Some(before) = pools.last()
            && before.sum * pool.weight > pool.sum * before.weight
        {
            pool = Pool { sum: before.sum + pool.sum, weight: before.weight + pool.weight, count: before.count + pool.count };
            pools.pop();
        }
        pools.push(pool);
    }
    pools
        .iter()
        .flat_map(|pool| {
            let start = (2 * pool.sum + pool.weight).div_euclid(2 * pool.weight);
            std::iter::repeat_n(i32::try_from(start).unwrap_or(i32::MAX), pool.count)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const APART: Apart = Apart { blocks: 4, links: 2 };

    fn block(breadth: i32) -> Width {
        Width { breadth, on_link: false }
    }

    /// The places of `widths` on `layers`, every link joining the middles of its ends.
    fn placed(layers: &[&[usize]], links: &[(usize, usize)], widths: &[Width]) -> Vec<i32> {
        let layers: Vec<Vec<usize>> = layers.iter().map(|layer| layer.to_vec()).collect();
        let port = |vertex: usize| (widths[vertex].breadth - 1) / 2;
        let segments: Vec<Segment> = links
            .iter()
            .map(|&(above, below)| Segment { above, below, weight: 1, above_port: port(above), below_port: port(below) })
            .collect();
        across(&layers, widths, &segments, APART)
    }

    #[test]
    fn a_chain_of_blocks_of_any_breadth_shares_one_middle() {
        assert_eq!(placed(&[&[0], &[1], &[2]], &[(0, 1), (1, 2)], &[block(9), block(5), block(12)]), [1, 3, 0]);
    }

    #[test]
    fn blocks_under_one_block_are_centred_on_it() {
        assert_eq!(placed(&[&[0], &[1, 2, 3]], &[(0, 1), (0, 2), (0, 3)], &[block(5); 4]), [9, 0, 9, 18]);
    }

    #[test]
    fn a_block_over_a_tree_is_centred_on_its_branches() {
        let links = [(0, 1), (0, 2), (1, 3), (1, 4), (2, 5), (2, 6)];

        assert_eq!(placed(&[&[0], &[1, 2], &[3, 4, 5, 6]], &links, &[block(3); 7]), [10, 3, 17, 0, 7, 14, 21]);
    }

    #[test]
    fn a_long_link_stays_straight_beside_the_blocks_it_passes() {
        let widths = [block(7), block(7), Width { breadth: 1, on_link: true }, block(7)];

        let places = placed(&[&[0], &[1, 2], &[3]], &[(0, 1), (0, 2), (1, 3), (2, 3)], &widths);

        assert_eq!(places[2], places[1] + 7 + APART.links);
    }

    #[test]
    fn a_segment_joining_a_block_off_its_middle_is_straight_there() {
        let layers = [vec![0], vec![1]];
        let segments = [Segment { above: 0, below: 1, weight: 1, above_port: 1, below_port: 8 }];

        assert_eq!(across(&layers, &[block(3), block(11)], &segments, APART), [7, 0]);
    }

    #[test]
    fn a_heavy_segment_is_straight_before_a_light_one() {
        let layers = [vec![0], vec![1, 2]];
        let segments = [
            Segment { above: 0, below: 1, weight: 4, above_port: 1, below_port: 1 },
            Segment { above: 0, below: 2, weight: 1, above_port: 1, below_port: 1 },
        ];

        assert_eq!(across(&layers, &[block(3); 3], &segments, APART), [0, 0, 7]);
    }

    #[test]
    fn starts_out_of_order_share_their_weighted_average() {
        let asked = [Asked { start: 10, weight: 1 }, Asked { start: 4, weight: 3 }, Asked { start: 20, weight: 1 }];

        assert_eq!(closest_in_order(&asked), [6, 6, 20]);
    }

    #[test]
    fn the_median_of_an_even_weight_lies_between_its_two_middle_values() {
        assert_eq!(median(&[(2, 1), (4, 1), (9, 1)]), Some(4));
        assert_eq!(median(&[(2, 1), (4, 1), (8, 1), (9, 1)]), Some(6));
        assert_eq!(median(&[(2, 1), (9, 3)]), Some(9));
        assert_eq!(median(&[]), None);
    }
}
