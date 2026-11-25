//! The lines of a drawing followed from glyph to glyph, to count what the glyphs alone miscount: the edges sharing
//! an arrowhead and the `┼` where a trunk parts or gathers.

use std::collections::{HashMap, HashSet};

pub(crate) const CROSSINGS: &str = "┼╋╪╫╬┿╀╁╂╃╄╅╆╇╈╉╊";
pub(crate) const TOP_LEFT: &str = "┌╭┏╔";
pub(crate) const TOP_RIGHT: &str = "┐╮┓╗";
pub(crate) const BOTTOM_LEFT: &str = "└╰┗╚";
pub(crate) const BOTTOM_RIGHT: &str = "┘╯┛╝";
pub(crate) const ARROWS: &str = "►▶▸▷→◄◀◂◁←▲▴△↑▼▾▽↓";
const VERTICAL: &str = "│┃║╎╏┆┇┊┋├┤┝┥┠┨┣┫╞╡╟╢╠╣┼╋╪╫╬";
const UPRIGHT: &str = "│┃║╎╏┆┇┊┋";
const LEVEL: &str = "─━═╌╍┄┅┈┉";
const TEE_RIGHT: &str = "├┝┠┣╞╟╠";
const TEE_LEFT: &str = "┤┥┨┫╡╢╣";
const TEE_DOWN: &str = "┬┯┰┳╤╥╦";
const TEE_UP: &str = "┴┷┸┻╧╨╩";

const UP: u8 = 1;
const RIGHT: u8 = 2;
const DOWN: u8 = 4;
const LEFT: u8 = 8;
const WAYS: [u8; 4] = [UP, RIGHT, DOWN, LEFT];

/// What following the lines adds to the glyph counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Lines {
    /// Edges that share an arrowhead with another: a fan-in merged into one line shows one arrowhead for all.
    pub(crate) shared_arrows: usize,
    /// `┼` where a trunk parts or gathers both ways: no two edges cross there.
    pub(crate) trunk_crossings: usize,
}

/// A closed rectangle of the drawing, a node or a frame, in rows and columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rectangle {
    top: usize,
    left: usize,
    bottom: usize,
    right: usize,
}

/// One line through a cell: the cell and, on a `┼`, whether it is the level line.
type Place = (usize, usize, bool);

/// Lines joined end to end, a `┼` joining only the cells it lies between: what one edge, or the edges of one trunk, draw.
#[derive(Debug, Default)]
struct Network {
    arrowheads: HashSet<(usize, usize)>,
    /// Ends on a box without an arrowhead: where an edge leaves its source.
    sources: usize,
    /// Ends on nothing: a label or a frame's title drawn across the line, or a shape that is no rectangle.
    loose: usize,
    /// The `┼` it runs through that are not on a box.
    crossings: Vec<Place>,
}

impl Network {
    /// A line across a trunk that only leads to arrowheads: the trunk parts both ways there.
    fn parts(&self) -> bool {
        self.arrowheads.len() > 1 && self.sources == 0 && self.loose == 0
    }

    /// A line that only leads to sources: across a line with an arrowhead, it gathers them into it.
    fn gathers(&self) -> bool {
        self.arrowheads.is_empty() && self.sources > 1 && self.loose == 0
    }
}

impl Lines {
    pub(crate) fn of(grid: &[Vec<char>], boxes: &[Rectangle]) -> Self {
        let borders: HashSet<(usize, usize)> = boxes.iter().flat_map(|found| found.border()).collect();
        let networks = networks(grid, &borders);
        let through: HashMap<Place, &Network> =
            networks.iter().flat_map(|network| network.crossings.iter().map(move |&place| (place, network))).collect();
        let pointed =
            |&(row, column, level): &Place| through.get(&(row, column, !level)).is_some_and(|across| !across.arrowheads.is_empty());
        let mut lines = Self { shared_arrows: 0, trunk_crossings: 0 };
        for network in &networks {
            let gathered: Vec<&Place> = network.crossings.iter().filter(|place| network.gathers() && pointed(place)).collect();
            if !network.arrowheads.is_empty() {
                lines.shared_arrows += network.sources.saturating_sub(network.arrowheads.len());
            } else if !gathered.is_empty() {
                lines.shared_arrows += network.sources;
            }
            lines.trunk_crossings += if network.parts() { network.crossings.len() } else { gathered.len() };
        }
        lines
    }
}

fn networks(grid: &[Vec<char>], borders: &HashSet<(usize, usize)>) -> Vec<Network> {
    let mut seen: HashSet<Place> = HashSet::new();
    let mut networks = Vec::new();
    for (row, cells) in grid.iter().enumerate() {
        for (column, &glyph) in cells.iter().enumerate() {
            for start in [false, true].map(|level| (row, column, level && CROSSINGS.contains(glyph))) {
                if ways(glyph) != 0 && !borders.contains(&(row, column)) && !seen.contains(&start) {
                    let (network, places) = network(grid, borders, start);
                    seen.extend(places);
                    networks.push(network);
                }
            }
        }
    }
    networks
}

impl Rectangle {
    fn border(self) -> impl Iterator<Item = (usize, usize)> {
        let rows = (self.left..=self.right).flat_map(move |column| [(self.top, column), (self.bottom, column)]);
        let columns = (self.top..=self.bottom).flat_map(move |row| [(row, self.left), (row, self.right)]);
        rows.chain(columns)
    }
}

/// Closed rectangles: a top-left corner whose row reaches a top-right corner, with sides down to the two bottom corners.
pub(crate) fn boxes(grid: &[Vec<char>]) -> Vec<Rectangle> {
    let is = |row: usize, column: usize, glyphs: &str| glyphs.contains(at(grid, row, column));
    let mut found = Vec::new();
    for (top, cells) in grid.iter().enumerate() {
        let lefts = cells.iter().enumerate().filter(|(_, cell)| TOP_LEFT.contains(**cell)).map(|(left, _)| left);
        for left in lefts {
            let Some(right) = (left + 1..cells.len()).find(|&column| is(top, column, TOP_RIGHT) || is(top, column, TOP_LEFT)) else {
                continue;
            };
            if !is(top, right, TOP_RIGHT) {
                continue;
            }
            let side = |column: usize| (top + 1..grid.len()).find(|&row| !is(row, column, VERTICAL));
            if let (Some(bottom), Some(other)) = (side(left), side(right))
                && bottom == other
                && is(bottom, left, BOTTOM_LEFT)
                && is(bottom, right, BOTTOM_RIGHT)
            {
                found.push(Rectangle { top, left, bottom, right });
            }
        }
    }
    found
}

/// What the lines joined to `start` end on, and every place they run through.
fn network(grid: &[Vec<char>], borders: &HashSet<(usize, usize)>, start: Place) -> (Network, HashSet<Place>) {
    let mut network = Network::default();
    let mut seen = HashSet::from([start]);
    let mut waiting = vec![start];
    while let Some((row, column, level)) = waiting.pop() {
        let glyph = at(grid, row, column);
        let crossing = CROSSINGS.contains(glyph);
        if crossing && !borders.contains(&(row, column)) {
            network.crossings.push((row, column, level));
        }
        for way in WAYS.into_iter().filter(|&way| ways(glyph) & way != 0 && (!crossing || is_level(way) == level)) {
            let Some((next_row, next_column)) = step(row, column, way) else {
                network.loose += 1;
                continue;
            };
            let next = at(grid, next_row, next_column);
            let passes = CROSSINGS.contains(next);
            if ARROWS.contains(next) {
                network.arrowheads.insert((next_row, next_column));
            } else if borders.contains(&(next_row, next_column)) && ways(next) != 0 && !passes {
                network.sources += 1;
            } else if ways(next) & opposite(way) == 0 {
                network.loose += 1;
            } else if seen.insert((next_row, next_column, passes && is_level(way))) {
                waiting.push((next_row, next_column, passes && is_level(way)));
            }
        }
    }
    (network, seen)
}

fn at(grid: &[Vec<char>], row: usize, column: usize) -> char {
    grid.get(row).and_then(|cells| cells.get(column)).copied().unwrap_or(' ')
}

/// The neighbours a line glyph reaches.
fn ways(glyph: char) -> u8 {
    let reaches = |glyphs: &str, ways: u8| if glyphs.contains(glyph) { ways } else { 0 };
    reaches(UPRIGHT, UP | DOWN)
        | reaches(LEVEL, LEFT | RIGHT)
        | reaches(TOP_LEFT, RIGHT | DOWN)
        | reaches(TOP_RIGHT, LEFT | DOWN)
        | reaches(BOTTOM_LEFT, UP | RIGHT)
        | reaches(BOTTOM_RIGHT, UP | LEFT)
        | reaches(TEE_RIGHT, UP | DOWN | RIGHT)
        | reaches(TEE_LEFT, UP | DOWN | LEFT)
        | reaches(TEE_DOWN, LEFT | RIGHT | DOWN)
        | reaches(TEE_UP, LEFT | RIGHT | UP)
        | reaches(CROSSINGS, UP | RIGHT | DOWN | LEFT)
}

const fn is_level(way: u8) -> bool {
    way == LEFT || way == RIGHT
}

const fn opposite(way: u8) -> u8 {
    match way {
        UP => DOWN,
        DOWN => UP,
        LEFT => RIGHT,
        _ => LEFT,
    }
}

fn step(row: usize, column: usize, way: u8) -> Option<(usize, usize)> {
    match way {
        UP => row.checked_sub(1).map(|row| (row, column)),
        DOWN => Some((row + 1, column)),
        LEFT => column.checked_sub(1).map(|column| (row, column)),
        _ => Some((row, column + 1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Lines {
        let grid: Vec<Vec<char>> = text.lines().map(|row| row.chars().collect()).collect();
        Lines::of(&grid, &boxes(&grid))
    }

    #[test]
    fn edges_merged_into_one_line_share_its_arrowhead() {
        let fan_in = "┌───┐\n│ A ├───┐\n└───┘   │\n┌───┐   │  ┌───┐\n│ B ├───┼─▸│ D │\n└───┘   │  └───┘\n┌───┐   │\n│ C ├───┘\n└───┘";

        assert_eq!(lines(fan_in), Lines { shared_arrows: 2, trunk_crossings: 1 });
    }

    #[test]
    fn a_trunk_parting_towards_its_own_arrowheads_shares_none() {
        let fan_out = "      ┌───▸│ B │\n┌───┐ │    └───┘\n│ A ├─┤\n└───┘ │    ┌───┐\n      └───▸│ C │";

        assert_eq!(lines(fan_out), Lines { shared_arrows: 0, trunk_crossings: 0 });
    }

    #[test]
    fn a_trunk_parting_both_ways_where_it_goes_on_is_a_split() {
        let split = "  ┌───┐\n  │ A │\n  └─┬─┘\n ┌──┼──┐\n ▾  ▾  ▾";

        assert_eq!(lines(split), Lines { shared_arrows: 0, trunk_crossings: 1 });
    }

    #[test]
    fn two_edges_crossing_are_no_split() {
        let crossing = "┌───┐  │\n│ A ├──┼──▸\n└───┘  ▾";

        assert_eq!(lines(crossing), Lines { shared_arrows: 0, trunk_crossings: 0 });
    }

    #[test]
    fn a_line_cut_by_its_label_is_no_split_and_shares_nothing() {
        let labelled = "  ┌───┐\n  │ A │\n  └─┬─┘\n ┌──┼─no─┐\n ▾  ▾    ▾";

        assert_eq!(lines(labelled), Lines { shared_arrows: 0, trunk_crossings: 0 });
    }

    #[test]
    fn an_edge_through_a_frame_is_one_line() {
        let framed = "┌───┐\n│ A │\n└─┬─┘\n╭─┼───╮\n│ ▾   │\n╰─────╯";

        assert_eq!(lines(framed), Lines { shared_arrows: 0, trunk_crossings: 0 });
    }
}
