/* Top-down frontier analysis for the query language, using `Catalog::op_cost` as the cost function */

use crate::lattice::{MeetLattice, WithTop};
use crate::querylang::{Catalog, CostProperties, QueryLang};
use crate::topdown::ComputeAnalysis;
use crate::upset::BackwardUpSet;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};

/// Newtype giving `CostProperties` the `WithTop` and `Hash` impls `BackwardUpSet` needs.
/// Order and equality are those of `CostProperties`; the hash covers the same fields as its `Eq`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
pub struct Cost(pub CostProperties);

impl Hash for Cost {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (&self.0.index, self.0.materialized, self.0.cost).hash(state);
    }
}

impl WithTop for Cost {
    fn top() -> Self {
        Cost(CostProperties {
            tables: HashSet::new(),
            cols: HashSet::new(),
            rows: usize::MAX,
            index: None,
            materialized: false,
            cost: usize::MAX,
        })
    }
}

/// The analysis domain: the frontier of non-dominated costs of an e-class.
/// Newtype so it can be `Hash`, which `BasicTopDownAnalysis` requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frontier(pub BackwardUpSet<Cost>);

impl Frontier {
    /// A threshold asking for a plan at least as good as (materialized, index, cost)
    pub fn threshold(materialized: bool, index: Option<&str>, cost: usize) -> Self {
        let mut t = Cost::top();
        t.0.materialized = materialized;
        t.0.index = index.map(String::from);
        t.0.cost = cost;
        Frontier(vec![t].into())
    }

    /// The frontier of a set of costs
    pub fn from_costs(costs: impl IntoIterator<Item = CostProperties>) -> Self {
        Frontier(BackwardUpSet::new(costs.into_iter().map(Cost).collect()))
    }

    /// The costs in the frontier, cheapest first
    pub fn costs(&self) -> Vec<&CostProperties> {
        let mut costs: Vec<_> = self.0.elements.iter().map(|c| &c.0).collect();
        costs.sort_by(|a, b| (a.cost, &a.index, a.materialized).cmp(&(b.cost, &b.index, b.materialized)));
        costs
    }
}

impl Hash for Frontier {
    /// Order-independent hash of the frontier elements
    fn hash<H: Hasher>(&self, state: &mut H) {
        let mut acc = 0u64;
        for e in &self.0.elements {
            let mut h = DefaultHasher::new();
            e.hash(&mut h);
            acc ^= h.finish();
        }
        acc.hash(state);
    }
}

impl PartialOrd for Frontier {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        self.0.partial_cmp(&o.0)
    }
}

impl WithTop for Frontier {
    fn top() -> Self {
        Frontier(BackwardUpSet::top())
    }
}

impl MeetLattice for Frontier {
    fn meet(&self, o: &Self) -> Self {
        Frontier(self.0.meet(&o.0))
    }
}

/// Lifts `Catalog::op_cost` to frontiers, counting how many times it is called
pub struct FrontierCost {
    pub catalog: Catalog,
    /// Number of `compute_analysis` calls, a measure of how much of the e-graph a query touched
    pub calls: usize,
}

impl FrontierCost {
    pub fn new(catalog: Catalog) -> Self {
        Self { catalog, calls: 0 }
    }
}

impl ComputeAnalysis<QueryLang, Frontier> for FrontierCost {
    /// Apply `op_cost` to every combination of one cost per child.
    fn compute_analysis(&mut self, enode: &QueryLang, children: &[Frontier]) -> Frontier {
        self.calls += 1;
        let mut combos: Vec<Vec<CostProperties>> = vec![vec![]];
        for child in children {
            combos = combos
                .iter()
                .flat_map(|combo| {
                    child.0.elements.iter().map(move |c| {
                        let mut next = combo.clone();
                        next.push(c.0.clone());
                        next
                    })
                })
                .collect();
        }
        Frontier::from_costs(combos.iter().map(|args| self.catalog.op_cost(enode, args)))
    }
}
