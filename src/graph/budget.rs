//! What the path searches of one drawing share: memory reused from one search to the next, and the steps every
//! placement tried takes out of one bound.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::error::Error;

/// The cost of a state no path of the search at hand reached.
pub(super) const UNREACHED: u32 = u32::MAX;

/// Each search's best costs and the state each was reached from, its queue, and the steps left.
#[derive(Debug, Clone)]
pub(super) struct Budget {
    cost: Vec<u32>,
    previous: Vec<usize>,
    /// The search a state's cost and previous state were set in: they mean nothing to any other.
    stamps: Vec<u32>,
    search: u32,
    queue: BinaryHeap<Reverse<(u32, usize)>>,
    steps_left: usize,
}

impl Budget {
    /// A budget of `steps` states taken off the search queues.
    pub(super) fn new(steps: usize) -> Self {
        Self { cost: Vec::new(), previous: Vec::new(), stamps: Vec::new(), search: 0, queue: BinaryHeap::new(), steps_left: steps }
    }

    /// Starts a search over `states` states, each unreached.
    pub(super) fn start(&mut self, states: usize) {
        if self.stamps.len() < states {
            self.cost.resize(states, UNREACHED);
            self.previous.resize(states, usize::MAX);
            self.stamps.resize(states, 0);
        }
        if self.search == u32::MAX {
            self.stamps.fill(0);
            self.search = 0;
        }
        self.search += 1;
        self.queue.clear();
    }

    pub(super) fn cost(&self, state: usize) -> u32 {
        if self.stamps[state] == self.search { self.cost[state] } else { UNREACHED }
    }

    /// The state `state` was reached from, none for a state the search started on.
    pub(super) fn previous(&self, state: usize) -> Option<usize> {
        Some(self.previous[state]).filter(|&previous| previous != usize::MAX && self.stamps[state] == self.search)
    }

    /// Records that `state` costs `cost` reached from `previous`, and queues it by `cost` plus the `estimate` left.
    pub(super) fn reach(&mut self, state: usize, cost: u32, previous: Option<usize>, estimate: u32) {
        self.cost[state] = cost;
        self.previous[state] = previous.unwrap_or(usize::MAX);
        self.stamps[state] = self.search;
        self.queue.push(Reverse((cost + estimate, state)));
    }

    /// The queued state with the lowest estimate and that estimate; refused once the steps are spent.
    pub(super) fn next(&mut self) -> Result<Option<(u32, usize)>, Error> {
        if self.queue.is_empty() {
            return Ok(None);
        }
        self.steps_left = self.steps_left.checked_sub(1).ok_or(Error::TooComplex)?;
        Ok(self.queue.pop().map(|Reverse(queued)| queued))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_search_finds_every_state_unreached() {
        let mut budget = Budget::new(10);
        budget.start(4);
        budget.reach(2, 7, Some(1), 0);

        budget.start(4);

        assert_eq!((budget.cost(2), budget.previous(2)), (UNREACHED, None));
    }

    #[test]
    fn states_come_off_the_queue_cheapest_estimate_first() {
        let mut budget = Budget::new(10);
        budget.start(4);
        budget.reach(0, 5, None, 5);
        budget.reach(1, 3, Some(0), 1);

        assert_eq!(budget.next(), Ok(Some((4, 1))));
        assert_eq!(budget.next(), Ok(Some((10, 0))));
        assert_eq!(budget.next(), Ok(None));
    }

    #[test]
    fn a_step_past_the_budget_is_refused_across_searches() {
        let mut budget = Budget::new(1);
        budget.start(2);
        budget.reach(0, 0, None, 0);
        assert_eq!(budget.next(), Ok(Some((0, 0))));

        budget.start(2);
        budget.reach(1, 0, None, 0);

        assert_eq!(budget.next(), Err(Error::TooComplex));
    }
}
