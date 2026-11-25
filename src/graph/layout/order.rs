//! Crossing reduction: the order of each layer's vertices that leaves the fewest links crossing.

/// Blocks and the dummies of the links passing a layer, each joined to its neighbours one layer up and one down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Layered {
    /// The vertices of each layer, in order across the flow.
    pub(super) layers: Vec<Vec<usize>>,
    pub(super) up: Vec<Vec<usize>>,
    pub(super) down: Vec<Vec<usize>>,
}

/// Sweeps tried at most; a flowchart a person writes settles in a handful.
const SWEEPS: usize = 24;
/// Sweeps in a row allowed to find nothing better.
const PATIENCE: usize = 4;
const TRANSPOSE_PASSES: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sweep {
    Down,
    Up,
}

/// The layers of `graph` reordered: sweeps down then up put each vertex at the average place of its neighbours on
/// the layer before, then swap neighbours wherever that uncrosses links; the order with the fewest crossings wins.
pub(super) fn ordered(graph: &Layered) -> Vec<Vec<usize>> {
    let mut layers = walked(graph);
    let mut fewest = crossings(graph, &layers);
    let mut best = layers.clone();
    let mut stale = 0;
    for sweep in (0..SWEEPS).map(|index| if index % 2 == 0 { Sweep::Down } else { Sweep::Up }) {
        if fewest == 0 || stale == PATIENCE {
            break;
        }
        layers = transposed(graph, swept(graph, layers, sweep));
        let count = crossings(graph, &layers);
        if count < fewest {
            (fewest, best, stale) = (count, layers.clone(), 0);
        } else {
            stale += 1;
        }
    }
    best
}

/// The first order: a walk down from each vertex in turn puts every vertex it meets next on its layer, so a
/// vertex starts near the one leading to it.
fn walked(graph: &Layered) -> Vec<Vec<usize>> {
    let mut layer_of = vec![0; graph.up.len()];
    for (index, layer) in graph.layers.iter().enumerate() {
        for &vertex in layer {
            layer_of[vertex] = index;
        }
    }
    let mut seen = vec![false; graph.up.len()];
    let mut layers: Vec<Vec<usize>> = graph.layers.iter().map(|layer| Vec::with_capacity(layer.len())).collect();
    for &root in graph.layers.iter().flatten() {
        let mut walk = vec![root];
        while let Some(vertex) = walk.pop() {
            if seen[vertex] {
                continue;
            }
            seen[vertex] = true;
            layers[layer_of[vertex]].push(vertex);
            walk.extend(graph.down[vertex].iter().rev().filter(|&&below| !seen[below]));
        }
    }
    layers
}

fn places(layers: &[Vec<usize>], vertices: usize) -> Vec<usize> {
    let mut places = vec![0; vertices];
    for layer in layers {
        for (place, &vertex) in layer.iter().enumerate() {
            places[vertex] = place;
        }
    }
    places
}

fn swept(graph: &Layered, layers: Vec<Vec<usize>>, sweep: Sweep) -> Vec<Vec<usize>> {
    let mut places = places(&layers, graph.up.len());
    let mut layers = layers;
    let (order, neighbours): (Vec<usize>, _) = match sweep {
        Sweep::Down => ((1..layers.len()).collect(), &graph.up),
        Sweep::Up => ((0..layers.len().saturating_sub(1)).rev().collect(), &graph.down),
    };
    for index in order {
        layers[index] = by_neighbours(&layers[index], neighbours, &places);
        for (place, &vertex) in layers[index].iter().enumerate() {
            places[vertex] = place;
        }
    }
    layers
}

/// The places of a vertex's neighbours, kept as a sum and a count so averages compare without rounding.
#[derive(Debug, Clone, Copy)]
struct Average {
    sum: usize,
    count: usize,
    vertex: usize,
}

/// `layer` sorted by the average place of each vertex's neighbours; a vertex without any keeps its place.
fn by_neighbours(layer: &[usize], neighbours: &[Vec<usize>], places: &[usize]) -> Vec<usize> {
    let sum = |vertex: usize| neighbours[vertex].iter().map(|&neighbour| places[neighbour]).sum::<usize>();
    let mut moving: Vec<Average> = layer
        .iter()
        .filter(|&&vertex| !neighbours[vertex].is_empty())
        .map(|&vertex| Average { sum: sum(vertex), count: neighbours[vertex].len(), vertex })
        .collect();
    moving.sort_by(|first, second| (first.sum * second.count).cmp(&(second.sum * first.count)));
    let mut moving = moving.into_iter().map(|average| average.vertex);
    layer.iter().map(|&vertex| if neighbours[vertex].is_empty() { vertex } else { moving.next().unwrap_or(vertex) }).collect()
}

/// Neighbours on a layer swapped as long as a swap leaves fewer links crossing.
fn transposed(graph: &Layered, layers: Vec<Vec<usize>>) -> Vec<Vec<usize>> {
    let mut places = places(&layers, graph.up.len());
    let mut layers = layers;
    for _ in 0..TRANSPOSE_PASSES {
        let mut swapped = false;
        for layer in &mut layers {
            for index in 1..layer.len() {
                let (left, right) = (layer[index - 1], layer[index]);
                if crossings_of_pair(graph, &places, right, left) < crossings_of_pair(graph, &places, left, right) {
                    layer.swap(index - 1, index);
                    places.swap(left, right);
                    swapped = true;
                }
            }
        }
        if !swapped {
            break;
        }
    }
    layers
}

/// Crossings between the links of `left` and those of `right` when `left` comes first.
fn crossings_of_pair(graph: &Layered, places: &[usize], left: usize, right: usize) -> usize {
    let crossed = |neighbours: &[Vec<usize>]| {
        neighbours[left].iter().map(|&near| neighbours[right].iter().filter(|&&other| places[other] < places[near]).count()).sum::<usize>()
    };
    crossed(&graph.up) + crossed(&graph.down)
}

/// Pairs of links between neighbouring layers that cross.
pub(super) fn crossings(graph: &Layered, layers: &[Vec<usize>]) -> usize {
    let places = places(layers, graph.up.len());
    layers
        .windows(2)
        .map(|pair| {
            let ends: Vec<usize> = pair[0]
                .iter()
                .flat_map(|&vertex| {
                    let mut ends: Vec<usize> = graph.down[vertex].iter().map(|&below| places[below]).collect();
                    ends.sort_unstable();
                    ends
                })
                .collect();
            inversions(&ends, pair[1].len())
        })
        .sum()
}

/// Pairs of `values`, each under `bound`, that come in falling order.
fn inversions(values: &[usize], bound: usize) -> usize {
    let mut counts = vec![0_usize; bound + 1];
    let mut inversions = 0;
    for (seen, &value) in values.iter().enumerate() {
        let mut at_most = 0;
        let mut index = value + 1;
        while index > 0 {
            at_most += counts[index];
            index -= index & index.wrapping_neg();
        }
        inversions += seen - at_most;
        let mut index = value + 1;
        while index <= bound {
            counts[index] += 1;
            index += index & index.wrapping_neg();
        }
    }
    inversions
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Layers of the given sizes, vertices numbered layer after layer, with `links` going one layer down.
    fn layered(sizes: &[usize], links: &[(usize, usize)]) -> Layered {
        let vertices: usize = sizes.iter().sum();
        let mut next = 0;
        let layers = sizes
            .iter()
            .map(|&size| {
                next += size;
                (next - size..next).collect()
            })
            .collect();
        let mut graph = Layered { layers, up: vec![Vec::new(); vertices], down: vec![Vec::new(); vertices] };
        for &(from, to) in links {
            graph.down[from].push(to);
            graph.up[to].push(from);
        }
        graph
    }

    #[test]
    fn links_in_falling_order_are_counted_as_crossings() {
        let graph = layered(&[2, 2], &[(0, 3), (1, 2)]);

        assert_eq!(crossings(&graph, &graph.layers), 1);
        assert_eq!(inversions(&[2, 0, 1, 1], 3), 3);
    }

    #[test]
    fn two_crossed_links_are_uncrossed() {
        let graph = layered(&[2, 2], &[(0, 3), (1, 2)]);

        let layers = ordered(&graph);

        assert_eq!(crossings(&graph, &layers), 0);
    }

    #[test]
    fn a_vertex_without_links_keeps_its_place_while_others_move_round_it() {
        let graph = layered(&[2, 3], &[(0, 4), (1, 2)]);

        assert_eq!(by_neighbours(&graph.layers[1], &graph.up, &[0, 1, 0, 0, 0]), [4, 3, 2]);
    }

    #[test]
    fn a_swap_of_neighbours_removes_a_crossing_averages_leave() {
        let graph = layered(&[2, 2, 2], &[(0, 2), (0, 3), (1, 3), (2, 5), (3, 4), (3, 5)]);

        let layers = ordered(&graph);

        assert_eq!(crossings(&graph, &layers), 0);
    }

    #[test]
    fn the_first_order_follows_the_links_down_from_each_vertex_in_turn() {
        let graph = layered(&[2, 3], &[(0, 4), (0, 2), (1, 3)]);

        assert_eq!(walked(&graph), [vec![0, 1], vec![4, 2, 3]]);
    }
}
