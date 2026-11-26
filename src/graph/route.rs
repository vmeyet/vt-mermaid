//! Edges as right-angled paths on the grid: a cheapest-path search that pays for each bend and crossing.

use std::collections::HashMap;
use std::ops::Range;

use vt_canvas::{Direction, Point, Rect};

use super::budget::Budget;
use super::diagram::{Diagram, End, Figure, GroupKind};
use super::layout::{Divider, Placement};
use super::port::{self, Join, Joins};
use super::shape;
use crate::error::Error;

/// A step; every other price is counted in steps, and what is left under one step only tells equal paths apart.
const STEP: u32 = 256;
const BEND: u32 = 4 * STEP;
const CROSSING: u32 = 6 * STEP;
/// Turning on the cell the arrow goes on, on top of the bend: a path with room turns before it.
const TURN_AT_THE_ARROW: u32 = 3 * STEP;
/// Leaving or entering a block through another side than the one facing the other end.
const OTHER_SIDE: u32 = 12 * STEP;
/// Running through the room kept for another edge's label, or across the room kept for its own.
const KEPT: u32 = 3 * STEP;
/// Cells of a straight leg at an end of a route its trunk's edges may leave or join it at.
const STUB: usize = 2;
/// Leaving or joining a trunk where it runs straight, which adds a junction; where it turns there is one already.
const JUNCTION: u32 = 3 * STEP;
/// The most a turn pays for standing away from where turns best stand.
const STRAY: u32 = 15;
/// Running right beside the border of a node the edge does not join, a cell: the line reads as the node's own.
const HUG: u32 = 5 * STEP / 2;
/// Times the edges are routed again, the edge left without a path going first.
const RETRIES: usize = 3;
/// Passing behind a frame's title, on top of crossing the frame.
const BEHIND_A_TITLE: u32 = 4 * STEP;
const HEADINGS: [Direction; 4] = [Direction::Up, Direction::Right, Direction::Down, Direction::Left];

/// An edge's cells, from the border cell it leaves its source through to the cell its arrow goes on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Route {
    pub(super) cells: Vec<Point>,
    /// The target's border cell next to the last cell.
    pub(super) end: Point,
}

impl Route {
    /// The direction the arrow points: from the last cell into the target.
    pub(super) fn arrival(&self) -> Direction {
        step_direction(self.cells.last().copied().unwrap_or(self.end), self.end)
    }

    /// The direction the edge leaves its source in.
    pub(super) fn departure(&self) -> Direction {
        match self.cells.as_slice() {
            [first, second, ..] => step_direction(*first, *second),
            _ => self.arrival(),
        }
    }

    /// The corners of the path: its ends and every cell where it turns.
    pub(super) fn corners(&self) -> Vec<Point> {
        let first = self.cells.first().copied();
        let last = self.cells.last().copied().filter(|_| self.cells.len() > 1);
        first.into_iter().chain(self.turns()).chain(last).collect()
    }

    /// The cells where the path turns.
    pub(super) fn turns(&self) -> impl Iterator<Item = Point> + '_ {
        self.cells.windows(3).filter(|cells| step_direction(cells[0], cells[1]) != step_direction(cells[1], cells[2])).map(|cells| cells[1])
    }

    /// The cells the path runs straight through, each with whether it runs sideways there.
    #[cfg(any(test, feature = "measure"))]
    pub(super) fn straight(&self) -> impl Iterator<Item = (Point, bool)> + '_ {
        let after = self.cells.iter().skip(1).chain([&self.end]);
        self.cells
            .windows(2)
            .zip(after.skip(1))
            .filter(|(cells, after)| step_direction(cells[0], cells[1]) == step_direction(cells[1], **after))
            .map(|(cells, _)| (cells[1], cells[0].y == cells[1].y))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Use {
    Free,
    /// A node, a frame corner or title, an edge's turn, arrow or crossing.
    Blocked,
    /// One straight line runs through, horizontally or not; another may cross it at a right angle.
    Line {
        horizontal: bool,
    },
    /// A straight line several edges of one trunk share: crossed like a line, never the place of a label.
    Trunk {
        horizontal: bool,
    },
    /// A node's border cell: edges join it, never pass it.
    Border,
    /// A node of one cell: any number of edges join it, from any side.
    Hub,
    /// A frame's top side under its title: an edge passes behind the title when no other cell will do.
    Title,
    /// Free, and kept for the label of an edge: the edge runs through it, horizontally or not, and anything else
    /// there pays.
    Kept {
        edge: u16,
        horizontal: bool,
    },
}

/// What each cell holds, for the search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Grid {
    pub(super) width: usize,
    pub(super) height: usize,
    pub(super) cells: Vec<Use>,
}

impl Grid {
    fn new(diagram: &Diagram, placement: &Placement, titles: &[Rect]) -> Self {
        let mut grid = Self::empty(placement.size.width, placement.size.height);
        for ((group, rect), title) in diagram.groups.iter().zip(&placement.groups).zip(titles) {
            if group.kind == GroupKind::Framed {
                grid.frame(*rect, *title);
            }
        }
        for divider in &placement.dividers {
            grid.divider(*divider);
        }
        for (node, rect) in diagram.nodes.iter().zip(&placement.nodes) {
            grid.node(*rect);
            if let Figure::Shape(shape) = node.figure {
                for row in shape::rules(*rect, shape, &node.label, &node.compartments) {
                    grid.set(Point::new(rect.x, row), Use::Blocked);
                    grid.set(Point::new(rect.x + rect.width.saturating_sub(1), row), Use::Blocked);
                }
            }
        }
        for (edge, (rect, end)) in
            placement.labels.iter().zip(&diagram.edges).enumerate().filter_map(|(edge, (rect, ends))| Some((edge, ((*rect)?, ends.from))))
        {
            let horizontal = matches!(diagram.flow_around(end), Direction::Left | Direction::Right);
            grid.fill(rect, Use::Kept { edge: to_u16(edge), horizontal });
        }
        grid
    }

    fn empty(width: u16, height: u16) -> Self {
        let (width, height) = (usize::from(width), usize::from(height));
        Self { width, height, cells: vec![Use::Free; width * height] }
    }

    pub(super) fn get(&self, at: Point) -> Use {
        self.index(at).map_or(Use::Blocked, |index| self.cells[index])
    }

    pub(super) fn set(&mut self, at: Point, cell: Use) {
        if let Some(index) = self.index(at) {
            self.cells[index] = cell;
        }
    }

    /// Frees the room still kept for the label of `edge`.
    pub(super) fn release(&mut self, edge: usize) {
        for cell in &mut self.cells {
            if matches!(*cell, Use::Kept { edge: kept, .. } if usize::from(kept) == edge) {
                *cell = Use::Free;
            }
        }
    }

    fn index(&self, at: Point) -> Option<usize> {
        let (x, y) = (usize::from(at.x), usize::from(at.y));
        (x < self.width && y < self.height).then_some(y * self.width + x)
    }

    fn fill(&mut self, rect: Rect, cell: Use) {
        for y in rect.y..rect.y + rect.height {
            for x in rect.x..rect.x + rect.width {
                self.set(Point::new(x, y), cell);
            }
        }
    }

    fn node(&mut self, rect: Rect) {
        if rect.width == 1 && rect.height == 1 {
            self.set(Point::new(rect.x, rect.y), Use::Hub);
            return;
        }
        self.fill(rect, Use::Border);
        self.fill(Rect::new(rect.x + 1, rect.y + 1, rect.width.saturating_sub(2), rect.height.saturating_sub(2)), Use::Blocked);
    }

    /// A frame's sides may be crossed, its corners may not, its title at a price.
    fn frame(&mut self, rect: Rect, title: Rect) {
        let (right, bottom) = (rect.x + rect.width - 1, rect.y + rect.height - 1);
        for x in rect.x..=right {
            self.set(Point::new(x, rect.y), Use::Line { horizontal: true });
            self.set(Point::new(x, bottom), Use::Line { horizontal: true });
        }
        for y in rect.y..=bottom {
            self.set(Point::new(rect.x, y), Use::Line { horizontal: false });
            self.set(Point::new(right, y), Use::Line { horizontal: false });
        }
        for corner in [Point::new(rect.x, rect.y), Point::new(right, rect.y), Point::new(rect.x, bottom), Point::new(right, bottom)] {
            self.set(corner, Use::Blocked);
        }
        self.fill(title, Use::Title);
    }

    /// A divider may be crossed like a frame's side, never where it meets the frame.
    fn divider(&mut self, divider: Divider) {
        let horizontal = divider.from.y == divider.to.y;
        let cells = Rect::new(divider.from.x, divider.from.y, divider.to.x - divider.from.x + 1, divider.to.y - divider.from.y + 1);
        self.fill(cells, Use::Line { horizontal });
        self.set(divider.from, Use::Blocked);
        self.set(divider.to, Use::Blocked);
    }

    /// Lays the cells of `route` it does not share on the grid so later routes only cross it at a right angle, away
    /// from its turns and ends; the cells it shares with its trunk are no place for a label any more, and no route
    /// crosses where it leaves or joins the trunk.
    fn add(&mut self, route: &Route, own: &Range<usize>, straight: Straight) {
        let last = route.cells.len() - 1;
        let tip = |index: usize| index <= usize::from(straight.from) || index + usize::from(straight.to) > last;
        for (index, &cell) in route.cells.iter().enumerate().skip(1) {
            let horizontal = route.cells[index - 1].y == cell.y;
            let turns = index < last && (route.cells[index + 1].y == cell.y) != horizontal;
            let shared = !own.contains(&index);
            let at_the_trunk = index + 1 == own.start || index == own.end;
            let next = match self.get(cell) {
                _ if tip(index) && index < last => Use::Blocked,
                Use::Free | Use::Kept { .. } if !turns && index < last => Use::Line { horizontal },
                Use::Line { horizontal } | Use::Trunk { horizontal } if shared && !at_the_trunk => Use::Trunk { horizontal },
                _ => Use::Blocked,
            };
            self.set(cell, next);
        }
        for &end in route.cells.first().into_iter().chain([&route.end]) {
            if self.get(end) != Use::Hub {
                self.set(end, Use::Blocked);
            }
        }
    }
}

/// A cell of a route the other edges of its trunk may leave or join it at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Tap {
    cell: Point,
    route: usize,
    /// The cell's place along the route.
    position: usize,
}

/// The cells edges sharing a way out of a block may leave each other at, and those edges sharing a way in may
/// join each other at, by trunk.
///
/// Along a route every edge leaving comes before any edge joining, so none reads as going from one to the other.
#[derive(Debug, Clone, Default)]
struct Trunks {
    leaving: HashMap<u16, Vec<Tap>>,
    joining: HashMap<u16, Vec<Tap>>,
    /// By route, the last place an edge leaves it at and the first place an edge joins it at.
    last_left: HashMap<usize, usize>,
    first_joined: HashMap<usize, usize>,
}

impl Trunks {
    /// Where an edge of `trunk` may still leave the routes sharing its way out.
    fn ways_off(&self, trunk: Option<u16>) -> Vec<Tap> {
        let taps = trunk.and_then(|trunk| self.leaving.get(&trunk)).map_or(&[][..], Vec::as_slice);
        taps.iter().filter(|tap| self.first_joined.get(&tap.route).is_none_or(|&joined| tap.position < joined)).copied().collect()
    }

    /// Where an edge of `trunk` may still join the routes sharing its way in.
    fn ways_in(&self, trunk: Option<u16>) -> Vec<Tap> {
        let taps = trunk.and_then(|trunk| self.joining.get(&trunk)).map_or(&[][..], Vec::as_slice);
        taps.iter().filter(|tap| self.last_left.get(&tap.route).is_none_or(|&left| tap.position > left)).copied().collect()
    }

    /// Notes where `found` leaves and joins earlier routes, then offers cells it laid itself to the edges of its
    /// trunks: to leave it at, the leg between its first two turns and the [`STUB`] cells before; to join it at,
    /// the leg between its last two turns and the cells after; so edges part right after their source and meet
    /// right before their target.
    fn add(&mut self, index: usize, found: &Found, joins: Joins) {
        if let Some(tap) = found.left {
            let last = self.last_left.entry(tap.route).or_insert(tap.position);
            *last = tap.position.max(*last);
        }
        if let Some(tap) = found.joined {
            let first = self.first_joined.entry(tap.route).or_insert(tap.position);
            *first = tap.position.min(*first);
        }
        let cells = &found.route.cells;
        let own = found.own.start.max(1)..found.own.end.min(cells.len() - 1);
        let turns = |position: &usize| {
            cells.get(position + 1).is_some_and(|after| (cells[position - 1].x == cells[*position].x) != (cells[*position].x == after.x))
        };
        let turns: Vec<usize> = own.clone().filter(turns).collect();
        let (first, last) = (turns.first().copied().unwrap_or(own.end), turns.last().copied().unwrap_or(own.start));
        let first_legs = own.start.max(first.saturating_sub(STUB))..turns.get(1).map_or(first, |turn| turn + 1).min(own.end);
        let first_legs = if turns.is_empty() { own.start..own.end.min(own.start + STUB) } else { first_legs };
        let last_legs = turns.iter().rev().nth(1).copied().unwrap_or(last)..own.end.min(last + 1 + STUB);
        let last_legs = if turns.is_empty() { own.start.max(own.end.saturating_sub(STUB))..own.end } else { last_legs };
        let taps = |legs: Range<usize>| legs.map(|position| Tap { cell: cells[position], route: index, position });
        if let Some(trunk) = joins.exit.trunk {
            self.leaving.entry(trunk).or_default().extend(taps(first_legs));
        }
        if let Some(trunk) = joins.entry.trunk {
            self.joining.entry(trunk).or_default().extend(taps(last_legs));
        }
    }
}

/// Every edge's route, the grid they leave, for the labels, and where each group's title goes on its frame.
#[derive(Debug, Clone)]
pub(super) struct Routed {
    pub(super) routes: Vec<Route>,
    pub(super) grid: Grid,
    /// The cells of each frame's top side its title takes with the line cell and the space on each side of it.
    pub(super) titles: Vec<Rect>,
}

/// Routes every edge, those between close blocks first, so the others go round what these already drew; an edge
/// left without a path goes first on another try; refused once the searches spend the budget.
pub(super) fn route(diagram: &Diagram, placement: &Placement, budget: &mut Budget) -> Result<Routed, Error> {
    let joins = port::joins(diagram, placement);
    let titles = titles(diagram, placement, &joins);
    let empty = Grid::new(diagram, placement, &titles);
    let mut order = nearest_first(diagram, placement);
    let mut stuck = None;
    for _ in 0..=RETRIES {
        if let Some(stuck) = stuck {
            order = std::iter::once(stuck).chain(order.into_iter().filter(|&index| index != stuck)).collect();
        }
        match laid(diagram, placement, &joins, &empty, &order, budget)? {
            Ok((routes, grid)) => return Ok(Routed { routes, grid, titles }),
            Err(index) if stuck == Some(index) => break,
            Err(index) => stuck = Some(index),
        }
    }
    let edge = stuck.map(|index| &diagram.edges[index]);
    let name = |end: Option<End>| end.map_or_else(String::new, |end| diagram.name(end).to_owned());
    Err(Error::Unroutable { from: name(edge.map(|edge| edge.from)), to: name(edge.map(|edge| edge.to)) })
}

/// The edges, those between close blocks first.
fn nearest_first(diagram: &Diagram, placement: &Placement) -> Vec<usize> {
    let rect = |end: End| match end {
        End::Node(index) => placement.nodes[index],
        End::Group(index) => placement.groups[index],
    };
    let mut order: Vec<usize> = (0..diagram.edges.len()).collect();
    order.sort_by_key(|&index| distance(rect(diagram.edges[index].from), rect(diagram.edges[index].to)));
    order
}

/// Every edge's route, searched in `order` on `empty`, and the grid they leave; else the first edge left without a
/// path; refused once the searches spend the budget.
fn laid(
    diagram: &Diagram,
    placement: &Placement,
    joins: &[Joins],
    empty: &Grid,
    order: &[usize],
    budget: &mut Budget,
) -> Result<Result<(Vec<Route>, Grid), usize>, Error> {
    let rect = |end: End| match end {
        End::Node(index) => placement.nodes[index],
        End::Group(index) => placement.groups[index],
    };
    let mut grid = empty.clone();
    let mut trunks = Trunks::default();
    let mut routes: Vec<Option<Route>> = vec![None; diagram.edges.len()];
    for &index in order {
        let edge = &diagram.edges[index];
        let twin = (0..routes.len()).find(|&other| routes[other].is_some() && twins(diagram, joins, index, other));
        let search = Search {
            grid: &grid,
            edge: to_u16(index),
            from: rect(edge.from),
            to: rect(edge.to),
            exit: joins[index].exit,
            entry: joins[index].entry,
            leaving: &trunks.ways_off(joins[index].exit.trunk),
            joining: &trunks.ways_in(joins[index].entry.trunk),
            routes: &routes,
            turns: if joins[index].entry.trunk.is_some() && joins[index].exit.trunk.is_none() { Turns::Late } else { Turns::Early },
            straight: Straight { from: edge.start.straight(), to: edge.end.straight() },
        };
        let shared = twin.and_then(|twin| routes[twin].clone()).map(|route| Found { route, own: 0..0, left: None, joined: None });
        let found = match shared {
            Some(found) => found,
            None => match search.run(budget)? {
                Some(found) => found,
                None => return Ok(Err(index)),
            },
        };
        grid.add(&found.route, &found.own, Straight { from: edge.start.straight(), to: edge.end.straight() });
        trunks.add(index, &found, joins[index]);
        routes[index] = Some(found.route);
    }
    Ok(Ok((routes.into_iter().flatten().collect(), grid)))
}

/// Whether two edges join the same blocks and share a trunk at both ends: they are then one line all the way.
fn twins(diagram: &Diagram, joins: &[Joins], edge: usize, other: usize) -> bool {
    let (ends, other_ends) = (&diagram.edges[edge], &diagram.edges[other]);
    let same_trunk = |join: Join, other: Join| join.trunk.is_some() && join.trunk == other.trunk;
    (ends.from, ends.to) == (other_ends.from, other_ends.to)
        && same_trunk(joins[edge].exit, joins[other].exit)
        && same_trunk(joins[edge].entry, joins[other].entry)
}

/// Where each group's title goes on the top side of its frame: after the corner, or further along when an edge is
/// best drawn straight down through there into a block of the group, or up out of one.
fn titles(diagram: &Diagram, placement: &Placement, joins: &[Joins]) -> Vec<Rect> {
    let rect = |end: End| match end {
        End::Node(index) => placement.nodes[index],
        End::Group(index) => placement.groups[index],
    };
    let within = |inner: Rect, outer: Rect| {
        outer.x < inner.x
            && outer.y < inner.y
            && inner.x + inner.width < outer.x + outer.width
            && inner.y + inner.height < outer.y + outer.height
    };
    let through_the_top = |frame: Rect| -> Vec<u16> {
        let crossing = |inside: Rect, outside: Rect, join: Join| {
            (join.cell.y == inside.y && within(inside, frame) && !within(outside, frame)).then_some(join.cell.x)
        };
        diagram
            .edges
            .iter()
            .zip(joins)
            .flat_map(|(edge, joins)| {
                let (from, to) = (rect(edge.from), rect(edge.to));
                [crossing(from, to, joins.exit), crossing(to, from, joins.entry)]
            })
            .flatten()
            .collect()
    };
    diagram
        .groups
        .iter()
        .zip(&placement.groups)
        .map(|(group, &frame)| {
            let width = group.title.width + 4;
            let wanted = through_the_top(frame);
            let free = |start: &u16| !wanted.iter().any(|x| (*start..start + width).contains(x));
            let start = (frame.x + 1..(frame.x + frame.width).saturating_sub(width)).find(free).unwrap_or(frame.x + 1);
            Rect::new(start, frame.y, width, 1)
        })
        .collect()
}

/// A route and the cells of it no earlier route laid: the others are its trunk's.
struct Found {
    route: Route,
    own: Range<usize>,
    /// Where it leaves an earlier route and where it joins one.
    left: Option<Tap>,
    joined: Option<Tap>,
}

/// Where a search ends: on a border cell of the target, or on a route of its trunk.
#[derive(Debug, Clone, Copy)]
enum Finish {
    Border(Point),
    Trunk(Tap),
}

/// One edge's search: from a border cell of `from`, out and round, into a border cell of `to`.
struct Search<'a> {
    grid: &'a Grid,
    edge: u16,
    from: Rect,
    to: Rect,
    /// The preferred way out of `from` and into `to`.
    exit: Join,
    entry: Join,
    /// Where it may leave the routes sharing its way out, and join those sharing its way in.
    leaving: &'a [Tap],
    joining: &'a [Tap],
    routes: &'a [Option<Route>],
    turns: Turns,
    straight: Straight,
}

/// The cells next to each end an edge runs straight through, without turning or crossing, so its tips stand on a
/// straight line no other edge crosses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Straight {
    from: u16,
    to: u16,
}

/// Where an edge turns when it may anywhere: right after its source, so edges leaving one block turn together and
/// a label has the rest of the way; or right before its target, when it shares its way in and not its way out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Turns {
    Early,
    Late,
}

impl Search<'_> {
    /// The cheapest route, none when no path joins the ends; refused once the searches spend the budget.
    fn run(&self, budget: &mut Budget) -> Result<Option<Found>, Error> {
        budget.start(self.grid.width * self.grid.height * 4);
        let mut best: Option<(u32, usize, Finish)> = None;
        let joining: HashMap<Point, Tap> = self.joining.iter().map(|tap| (tap.cell, *tap)).collect();
        let target = self.joining.iter().fold(self.to, |target, tap| around(target, tap.cell));
        let ports = self.ports_of(self.from).map(|(cell, outward)| (cell, outward, away(self.from, self.exit, cell, outward)));
        for (cell, heading, price) in ports.chain(self.leaving.iter().flat_map(|tap| self.ways_off(*tap))) {
            let state = self.state(cell, heading);
            if price < budget.cost(state) {
                budget.reach(state, price, None, STEP * estimate(target, cell));
            }
        }
        while let Some((estimate_through, state)) = budget.next()? {
            if best.is_some_and(|(total, ..)| estimate_through >= total) {
                break;
            }
            let (cell, heading) = self.cell_of(state);
            for direction in HEADINGS {
                let Some(next) = neighbour(cell, direction).filter(|_| direction != opposite(heading)) else { continue };
                let finish = match joining.get(&next) {
                    Some(&tap) => self.joining_cost(cell, heading, direction, tap).map(|price| (price, Finish::Trunk(tap))),
                    None => self.arrival(cell, heading, next, direction).map(|price| (STEP + price, Finish::Border(next))),
                };
                if let Some((price, finish)) = finish {
                    let total = budget.cost(state) + price;
                    if best.is_none_or(|(known, ..)| total < known) {
                        best = Some((total, state, finish));
                    }
                    continue;
                }
                let Some(step) = self.step_cost(cell, heading, next, direction) else { continue };
                let next_state = self.state(next, direction);
                let next_cost = budget.cost(state) + step;
                if next_cost < budget.cost(next_state) {
                    budget.reach(next_state, next_cost, Some(state), STEP * estimate(target, next));
                }
            }
        }
        Ok(best.map(|(_, state, finish)| self.found(self.path(budget, state), finish)))
    }

    /// The whole route: the trunk up to where the searched `path` leaves it, the path, then the trunk it joins.
    fn found(&self, path: Vec<Point>, finish: Finish) -> Found {
        let shared = |tap: &Tap| self.routes[tap.route].as_ref().map(|route| (route, tap.position));
        let left = path.first().and_then(|first| self.leaving.iter().find(|tap| tap.cell == *first)).copied();
        let mut cells: Vec<Point> =
            left.as_ref().and_then(shared).map_or_else(Vec::new, |(route, position)| route.cells[..position].to_vec());
        let own = cells.len()..cells.len() + path.len();
        cells.extend(path);
        let (end, joined) = match finish {
            Finish::Border(end) => (end, None),
            Finish::Trunk(tap) => match shared(&tap) {
                Some((route, position)) => {
                    cells.extend(&route.cells[position..]);
                    (route.end, Some(tap))
                }
                None => (tap.cell, None),
            },
        };
        Found { route: Route { cells, end }, own, left, joined }
    }

    /// The ways a route may leave its trunk at `tap`, each with its price: any way but along the trunk, for
    /// nothing where the trunk turns already, else for the junction it adds.
    fn ways_off(&self, tap: Tap) -> impl Iterator<Item = (Point, Direction, u32)> + '_ {
        let along = self.routes[tap.route].as_ref().and_then(|route| directions_at(route, tap.position));
        let past_the_tips = !within(tap.cell, self.from, self.straight.from);
        HEADINGS.into_iter().filter_map(move |heading| {
            let (arriving, leaving) = along.filter(|_| past_the_tips)?;
            let free = heading != leaving && heading != opposite(arriving);
            free.then_some((tap.cell, heading, if arriving == leaving { JUNCTION } else { 0 }))
        })
    }

    /// What joining the trunk at `tap` costs, coming from `cell`, a free cell; never among the cells kept straight for
    /// its tips.
    fn joining_cost(&self, cell: Point, heading: Direction, direction: Direction, tap: Tap) -> Option<u32> {
        if within(tap.cell, self.to, self.straight.to) {
            return None;
        }
        let turning = match (direction != heading, self.grid.get(cell)) {
            (false, Use::Free | Use::Kept { .. }) => 0,
            (true, Use::Free) => BEND + self.stray(cell),
            (true, Use::Kept { .. }) => BEND + KEPT + self.stray(cell),
            _ => return None,
        };
        let (arriving, leaving) = self.routes[tap.route].as_ref().and_then(|route| directions_at(route, tap.position))?;
        Some(STEP + turning + if arriving == leaving { JUNCTION } else { 0 })
    }

    /// What arriving costs when `next` is a free border cell of the target, entered from a free cell.
    ///
    /// The arrow goes on that cell: a line may turn there into `▾` or `▴` (`──▾` reads well), never into `▸` or `◂`,
    /// which a vertical line beside them would hide.
    fn arrival(&self, cell: Point, heading: Direction, next: Point, direction: Direction) -> Option<u32> {
        let inward = sides(self.to, next).any(|side| side == opposite(direction)) && self.joinable(next);
        let turn = match (direction == heading, direction) {
            (true, _) => Some(0),
            (false, _) if self.straight.to > 0 => None,
            (false, Direction::Down | Direction::Up) => Some(BEND + TURN_AT_THE_ARROW),
            (false, _) => None,
        };
        let cost = turn.filter(|_| inward && matches!(self.grid.get(cell), Use::Free | Use::Kept { .. }))?;
        Some(cost + away(self.to, self.entry, next, direction))
    }

    /// What moving from `cell` to `next` costs, `None` where it may not go.
    fn step_cost(&self, cell: Point, heading: Direction, next: Point, direction: Direction) -> Option<u32> {
        if (direction != heading && self.at_tips(cell))
            || (self.at_tips(next) && !matches!(self.grid.get(next), Use::Free | Use::Kept { .. }))
        {
            return None;
        }
        let turning = match (direction != heading, self.grid.get(cell)) {
            (false, _) => 0,
            (true, Use::Free) => BEND + self.stray(cell),
            (true, Use::Kept { .. }) => BEND + KEPT + self.stray(cell),
            (true, _) => return None,
        };
        let horizontal = matches!(direction, Direction::Left | Direction::Right);
        let entering = match self.grid.get(next) {
            Use::Free => STEP,
            Use::Kept { edge, horizontal: line } if edge == self.edge && line == horizontal => STEP,
            Use::Kept { .. } => STEP + KEPT,
            Use::Line { horizontal: line } | Use::Trunk { horizontal: line } if line != horizontal => STEP + CROSSING,
            Use::Title if !horizontal => STEP + CROSSING + BEHIND_A_TITLE,
            _ => return None,
        };
        Some(entering + turning + self.hugging(next, horizontal))
    }

    /// What running through `cell` right beside a node that is not one of the edge's ends costs: a line there
    /// hides the node's side from the edges joining it.
    fn hugging(&self, cell: Point, horizontal: bool) -> u32 {
        let beside = if horizontal { [Direction::Up, Direction::Down] } else { [Direction::Left, Direction::Right] };
        let own = |cell: Point| {
            [self.from, self.to]
                .into_iter()
                .any(|rect| (rect.x..rect.x + rect.width).contains(&cell.x) && (rect.y..rect.y + rect.height).contains(&cell.y))
        };
        let hugs =
            beside.into_iter().filter_map(|side| neighbour(cell, side)).any(|beside| self.grid.get(beside) == Use::Border && !own(beside));
        if hugs { HUG } else { 0 }
    }

    /// How far a turn at `cell` stands from where this edge's turns read best: two cells off the block they stand
    /// by sideways, one below or above a block left, two before a block joined, which leaves the arrow a cell.
    fn stray(&self, cell: Point) -> u32 {
        let gap = |value: u16, start: u16, length: u16| {
            u32::from(start.saturating_sub(value)) + u32::from(value.saturating_sub(start + length - 1))
        };
        let off = |gap: u32, best: u32| if gap == 0 { 0 } else { gap.abs_diff(best) };
        let (near, upright) = match self.turns {
            Turns::Early => (self.from, 1),
            Turns::Late => (self.to, 2),
        };
        (off(gap(cell.x, near.x, near.width), 2) + off(gap(cell.y, near.y, near.height), upright)).min(STRAY)
    }

    /// Whether `cell` is among those kept straight next to either end: a turn or a crossing there would break its
    /// tips.
    fn at_tips(&self, cell: Point) -> bool {
        within(cell, self.from, self.straight.from) || within(cell, self.to, self.straight.to)
    }

    /// The border cells of `rect` an edge may still join, each with the direction leading out.
    fn ports_of(&self, rect: Rect) -> impl Iterator<Item = (Point, Direction)> + '_ {
        let (right, bottom) = (rect.x + rect.width - 1, rect.y + rect.height - 1);
        let rows = (rect.x..=right).flat_map(move |x| [Point::new(x, rect.y), Point::new(x, bottom)]);
        let columns = (rect.y..=bottom).flat_map(move |y| [Point::new(rect.x, y), Point::new(right, y)]);
        rows.chain(columns).filter(|cell| self.joinable(*cell)).flat_map(move |cell| sides(rect, cell).map(move |outward| (cell, outward)))
    }

    /// A node's border or a frame's side no earlier edge joins; under a frame's title too, which the arrow stays clear of.
    fn joinable(&self, cell: Point) -> bool {
        matches!(self.grid.get(cell), Use::Border | Use::Hub | Use::Line { .. } | Use::Trunk { .. } | Use::Title)
    }

    fn path(&self, budget: &Budget, last: usize) -> Vec<Point> {
        let mut cells: Vec<Point> =
            std::iter::successors(Some(last), |&state| budget.previous(state)).map(|state| self.cell_of(state).0).collect();
        cells.reverse();
        cells
    }

    fn state(&self, cell: Point, heading: Direction) -> usize {
        let heading = HEADINGS.iter().position(|&known| known == heading).unwrap_or(0);
        (usize::from(cell.y) * self.grid.width + usize::from(cell.x)) * 4 + heading
    }

    fn cell_of(&self, state: usize) -> (Point, Direction) {
        let index = state / 4;
        let heading = HEADINGS[state % 4];
        (Point::new(to_u16(index % self.grid.width), to_u16(index / self.grid.width)), heading)
    }
}

/// What going through `cell` of `rect`'s border `heading` costs: twice the cells from the join it is best drawn with
/// on the same side, else the price of another side and of how far from that side's middle it is.
fn away(rect: Rect, best: Join, cell: Point, heading: Direction) -> u32 {
    if heading == best.heading {
        2 * STEP * u32::from(cell.x.abs_diff(best.cell.x) + cell.y.abs_diff(best.cell.y))
    } else {
        OTHER_SIDE + STEP * off_centre(rect, cell)
    }
}

/// Half the distance from `cell` to the middle of its side of `rect`, so edges join nodes near the middle.
fn off_centre(rect: Rect, cell: Point) -> u32 {
    let distance = |value: u16, start: u16, length: u16| (u32::from(value) * 2).abs_diff(u32::from(start) * 2 + u32::from(length) - 1);
    let offset = if cell.y == rect.y || cell.y == rect.y + rect.height - 1 {
        distance(cell.x, rect.x, rect.width)
    } else {
        distance(cell.y, rect.y, rect.height)
    };
    (offset + 2) / 4
}

/// The directions out of `rect` through `cell`, a border cell that is not a corner: one for a block of three rows
/// and columns or more, both ways across a bar one cell thick, every way out of a single cell.
fn sides(rect: Rect, cell: Point) -> impl Iterator<Item = Direction> {
    let (right, bottom) = (rect.x + rect.width - 1, rect.y + rect.height - 1);
    let across = (rect.x + 1..right).contains(&cell.x) || (rect.width == 1 && cell.x == rect.x);
    let down = (rect.y + 1..bottom).contains(&cell.y) || (rect.height == 1 && cell.y == rect.y);
    let up = across && cell.y == rect.y;
    let thick = rect.height > 1 && rect.width > 1;
    [
        (up, Direction::Up),
        (across && cell.y == bottom && !(up && thick), Direction::Down),
        (down && cell.x == rect.x, Direction::Left),
        (down && cell.x == right && !(cell.x == rect.x && thick), Direction::Right),
    ]
    .into_iter()
    .filter_map(|(out, direction)| out.then_some(direction))
}

/// Whether `cell` lies outside `rect`, `cells` or fewer away from it, diagonally included.
fn within(cell: Point, rect: Rect, cells: u16) -> bool {
    let gap = |value: u16, start: u16, length: u16| start.saturating_sub(value).max(value.saturating_sub(start + length - 1));
    let away = gap(cell.x, rect.x, rect.width).max(gap(cell.y, rect.y, rect.height));
    cells > 0 && (1..=cells).contains(&away)
}

/// Steps from `cell` to the nearest cell of `target`, never more than a path takes.
fn estimate(target: Rect, cell: Point) -> u32 {
    let gap =
        |value: u16, start: u16, length: u16| u32::from(start.saturating_sub(value)) + u32::from(value.saturating_sub(start + length - 1));
    gap(cell.x, target.x, target.width) + gap(cell.y, target.y, target.height)
}

/// The smallest rectangle holding `rect` and `cell`.
fn around(rect: Rect, cell: Point) -> Rect {
    let (left, top) = (rect.x.min(cell.x), rect.y.min(cell.y));
    let (right, bottom) = ((rect.x + rect.width).max(cell.x + 1), (rect.y + rect.height).max(cell.y + 1));
    Rect::new(left, top, right - left, bottom - top)
}

/// The directions `route` arrives at and leaves the cell at `position` in.
fn directions_at(route: &Route, position: usize) -> Option<(Direction, Direction)> {
    let arriving = step_direction(*route.cells.get(position.checked_sub(1)?)?, route.cells[position]);
    let leaving = step_direction(route.cells[position], route.cells.get(position + 1).copied().unwrap_or(route.end));
    Some((arriving, leaving))
}

fn neighbour(cell: Point, direction: Direction) -> Option<Point> {
    match direction {
        Direction::Up => cell.y.checked_sub(1).map(|y| Point::new(cell.x, y)),
        Direction::Right => cell.x.checked_add(1).map(|x| Point::new(x, cell.y)),
        Direction::Down => cell.y.checked_add(1).map(|y| Point::new(cell.x, y)),
        Direction::Left => cell.x.checked_sub(1).map(|x| Point::new(x, cell.y)),
    }
}

pub(super) fn step_direction(from: Point, to: Point) -> Direction {
    if to.x > from.x {
        Direction::Right
    } else if to.x < from.x {
        Direction::Left
    } else if to.y > from.y {
        Direction::Down
    } else {
        Direction::Up
    }
}

pub(super) const fn opposite(direction: Direction) -> Direction {
    match direction {
        Direction::Up => Direction::Down,
        Direction::Right => Direction::Left,
        Direction::Down => Direction::Up,
        Direction::Left => Direction::Right,
    }
}

fn distance(from: Rect, to: Rect) -> u32 {
    let centre = |rect: Rect| (u32::from(rect.x) * 2 + u32::from(rect.width), u32::from(rect.y) * 2 + u32::from(rect.height));
    let ((x1, y1), (x2, y2)) = (centre(from), centre(to));
    x1.abs_diff(x2) + y1.abs_diff(y2)
}

fn to_u16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flowchart;
    use crate::graph::edge_label::EndLabels;
    use crate::graph::layout::{self, Fit};
    use crate::graph::{draw, measure};

    #[test]
    fn a_bar_is_left_both_ways_across_and_a_single_cell_every_way() {
        let directions = |rect: Rect, cell: Point| sides(rect, cell).collect::<Vec<_>>();

        assert_eq!(directions(Rect::new(2, 2, 5, 1), Point::new(4, 2)), [Direction::Up, Direction::Down]);
        assert_eq!(directions(Rect::new(2, 2, 5, 1), Point::new(2, 2)), [Direction::Left]);
        assert_eq!(
            directions(Rect::new(2, 2, 1, 1), Point::new(2, 2)),
            [Direction::Up, Direction::Down, Direction::Left, Direction::Right]
        );
        assert_eq!(directions(Rect::new(2, 2, 1, 1), Point::new(2, 5)), []);
        assert_eq!(directions(Rect::new(2, 2, 5, 3), Point::new(4, 4)), [Direction::Down]);
    }

    const TOP: Rect = Rect::new(2, 0, 7, 3);
    const BOTTOM: Rect = Rect::new(2, 8, 7, 3);

    fn grid(nodes: &[Rect]) -> Grid {
        let mut grid = Grid::empty(20, 12);
        for node in nodes {
            grid.node(*node);
        }
        grid
    }

    fn search(grid: &Grid, from: Rect, to: Rect) -> Option<Route> {
        search_straight(grid, from, to, Straight::default())
    }

    fn search_straight(grid: &Grid, from: Rect, to: Rect, straight: Straight) -> Option<Route> {
        let exit = Join { heading: Direction::Down, cell: Point::new(from.x + from.width / 2, from.y + from.height - 1), trunk: None };
        let entry = Join { heading: Direction::Down, cell: Point::new(to.x + to.width / 2, to.y), trunk: None };
        Search { grid, edge: 0, from, to, exit, entry, leaving: &[], joining: &[], routes: &[], turns: Turns::Early, straight }
            .run(&mut Budget::new(usize::MAX))
            .unwrap()
            .map(|found| found.route)
    }

    #[test]
    fn an_edge_leaves_down_the_middle_and_enters_from_the_top() {
        let route = search(&grid(&[TOP, BOTTOM]), TOP, BOTTOM).unwrap();

        assert_eq!(route.corners(), [Point::new(5, 2), Point::new(5, 7)]);
        assert_eq!((route.departure(), route.arrival(), route.end), (Direction::Down, Direction::Down, Point::new(5, 8)));
    }

    #[test]
    fn an_edge_with_tips_runs_straight_through_the_cells_next_to_both_ends() {
        let (from, to) = (Rect::new(0, 0, 5, 3), Rect::new(10, 8, 5, 3));

        let bent = search(&grid(&[from, to]), from, to).unwrap();
        let straight = search_straight(&grid(&[from, to]), from, to, Straight { from: 2, to: 2 }).unwrap();

        let column = |route: &Route, cells: Range<usize>| route.cells[cells].iter().map(|cell| cell.x).collect::<Vec<_>>();
        let last = straight.cells.len();
        assert_ne!(column(&bent, 0..3), [2, 2, 2], "{:?}", bent.cells);
        assert_eq!((column(&straight, 0..3), column(&straight, last - 2..last)), (vec![2, 2, 2], vec![12, 12]), "{:?}", straight.cells);
        assert_eq!(straight.arrival(), Direction::Down);
    }

    #[test]
    fn an_edge_goes_round_a_node_in_its_way() {
        let middle = Rect::new(0, 4, 12, 3);

        let route = search(&grid(&[TOP, BOTTOM, middle]), TOP, BOTTOM).unwrap();

        assert!(route.cells.iter().all(|cell| cell.x >= 12 || cell.y < 4 || cell.y > 6));
    }

    #[test]
    fn an_edge_keeps_a_cell_off_a_node_it_passes() {
        let beside = Rect::new(6, 3, 6, 5);

        let route = search(&grid(&[TOP, BOTTOM, beside]), TOP, BOTTOM).unwrap();

        assert_eq!(route.corners(), [Point::new(4, 2), Point::new(4, 7)]);
    }

    #[test]
    fn an_edge_crosses_a_line_only_at_a_right_angle() {
        let mut crossed = grid(&[TOP, BOTTOM]);
        crossed.fill(Rect::new(0, 5, 20, 1), Use::Line { horizontal: true });
        let mut parallel = grid(&[TOP, BOTTOM]);
        parallel.fill(Rect::new(5, 3, 1, 5), Use::Line { horizontal: false });

        let across = search(&crossed, TOP, BOTTOM).unwrap();
        let beside = search(&parallel, TOP, BOTTOM).unwrap();

        assert!(across.cells.contains(&Point::new(5, 5)));
        assert!(!beside.cells.iter().any(|cell| cell.x == 5 && (3..8).contains(&cell.y)));
    }

    #[test]
    fn a_route_on_the_grid_blocks_its_turns_and_ends_and_lines_the_rest() {
        let mut grid = grid(&[TOP, BOTTOM]);
        let route = Route { cells: vec![Point::new(5, 2), Point::new(5, 3), Point::new(6, 3), Point::new(7, 3)], end: Point::new(8, 3) };

        grid.add(&route, &(0..4), Straight::default());

        let uses = [(5, 2), (5, 3), (6, 3), (7, 3), (8, 3)].map(|(x, y)| grid.get(Point::new(x, y)));
        assert_eq!(uses, [Use::Blocked, Use::Blocked, Use::Line { horizontal: true }, Use::Blocked, Use::Blocked]);
    }

    fn routed(source: &str) -> (Routed, Vec<String>) {
        let diagram = flowchart::read(source).unwrap();
        let placement = layout::place(&diagram, Fit::Loose);
        let routed = route(&diagram, &placement, &mut Budget::new(usize::MAX)).unwrap();
        let labels = vec![None; routed.routes.len()];
        let end_labels = vec![EndLabels::default(); routed.routes.len()];
        let parts = draw::Parts {
            nodes: placement.nodes,
            groups: placement.groups,
            titles: routed.titles.clone(),
            dividers: placement.dividers,
            routes: routed.routes.clone(),
            labels,
            end_labels,
        };
        let rows = measure::rows(&draw::draw(&diagram, parts, 200).unwrap().canvas);
        (routed, rows)
    }

    #[test]
    fn edges_into_a_side_with_one_cell_meet_on_a_trunk_right_before_it() {
        let (routed, rows) = routed("flowchart LR\n  A --> D\n  B --> D\n  C --> D");

        assert_eq!(&rows[1..6], ["│ A ├───┐       ", "└───┘   │       ", "        │       ", "┌───┐   │  ┌───┐", "│ B ├───┼─▸│ D │"]);
        assert_eq!(rows[9], "│ C ├───┘       ");
        assert!(routed.routes.iter().all(|route| route.end == routed.routes[0].end));
    }

    #[test]
    fn edges_out_of_a_side_with_one_cell_part_on_a_trunk_right_after_it() {
        let (routed, rows) = routed("flowchart LR\n  A --> B & C");

        assert_eq!(&rows[1..6], ["      ┌───▸│ B │", "┌───┐ │    └───┘", "│ A ├─┤         ", "└───┘ │    ┌───┐", "      └───▸│ C │"]);
        assert_eq!(routed.routes[0].cells[..3], routed.routes[1].cells[..3]);
    }

    #[test]
    fn twin_edges_sharing_a_trunk_at_both_ends_are_one_line() {
        let (routed, _) = routed("flowchart LR\n  A --- B\n  A --- B");

        assert_eq!(routed.routes[0], routed.routes[1]);
        assert!(routed.grid.cells.contains(&Use::Trunk { horizontal: true }));
    }

    #[test]
    fn an_edge_turns_before_the_room_kept_for_its_label_and_runs_through_it() {
        let (_, rows) = routed("flowchart TB\n  A -->|a long label| B\n  A -->|another one| C");

        assert_eq!(&rows[2..6], ["       └┬─┬┘       ", "  ┌─────┘ └─────┐  ", "  │             │  ", "  ▾             ▾  "]);
    }

    #[test]
    fn a_group_title_moves_off_the_column_an_edge_comes_down_through() {
        let source = "flowchart TB\n  A --> B[A node wider than the title]\n  subgraph G [Title]\n    B\n  end";

        let (routed, rows) = routed(source);

        assert_eq!(rows[6], "╭─ Title ────────┼────────────────╮");
        assert_eq!(routed.titles, [Rect::new(4, 9, 9, 1)]);
    }

    #[test]
    fn an_edge_passes_behind_a_title_as_wide_as_its_frame() {
        let (_, rows) = routed("flowchart TB\n  A --> B\n  subgraph G [Group title]\n    B\n  end");

        assert_eq!(&rows[5..8], ["    │            ", "╭─ Group title ─╮", "│   ▾           │"]);
    }

    #[test]
    fn an_edge_left_without_a_path_goes_first_on_another_try() {
        let source =
            include_str!("../../corpus/flowchart/e2e--platform-dev-diagrams-tmp-v12-0-0-faulty-diags-elk-legacy-preset-regression.mmd");
        let diagram = flowchart::read(source).unwrap().turned_top_down();
        let placement = layout::place(&diagram, Fit::Packed { columns: 50 });
        let joins = port::joins(&diagram, &placement);
        let empty = Grid::new(&diagram, &placement, &titles(&diagram, &placement, &joins));

        let nearest_first = laid(&diagram, &placement, &joins, &empty, &nearest_first(&diagram, &placement), &mut Budget::new(usize::MAX));

        assert!(matches!(nearest_first, Ok(Err(_))));
        assert!(route(&diagram, &placement, &mut Budget::new(usize::MAX)).is_ok());
    }

    #[test]
    fn routing_past_the_budget_is_refused() {
        let diagram = flowchart::read("flowchart TB\n  A --> B").unwrap();
        let placement = layout::place(&diagram, Fit::Loose);

        assert!(matches!(route(&diagram, &placement, &mut Budget::new(3)), Err(Error::TooComplex)));
    }

    #[test]
    fn an_edge_to_a_group_joins_its_frame_under_the_title() {
        let (_, rows) = routed("flowchart TB\n  A --> G\n  subgraph G [Group title]\n    B\n  end");

        assert_eq!(&rows[4..7], ["        │        ", "        ▾        ", "╭─ Group title ─╮"]);
    }

    #[test]
    fn a_target_walled_in_cannot_be_reached() {
        let mut walled = grid(&[TOP, BOTTOM]);
        walled.fill(Rect::new(0, 6, 20, 1), Use::Blocked);

        assert_eq!(search(&walled, TOP, BOTTOM), None);
    }
}
