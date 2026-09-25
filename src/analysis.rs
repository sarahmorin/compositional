/* Frontier Analysis */

use crate::querylang::*;
use egg::*;

/// MulteNodes are nodes whose arguments are annotated with cost properties.
/// They are used to maintain a non-dominated frontier of nodes per e-class.
#[derive(Debug, Clone)]
pub struct MulteNode<L: Language> {
    id: Id,
    node: L,
    args: Vec<(Id, CostProperties)>,
    cost: CostProperties,
}

impl MulteNode<QueryLang> {}

// TODO: implement the e-class analysis that maintains a non-dominated frontier of multe-nodes per class
#[derive(Debug, Clone)]
struct FrontierAnalysis(Vec<MulteNode<QueryLang>>, Catalog);

impl FrontierAnalysis {
    fn new(catalog: Catalog) -> Self {
        FrontierAnalysis(vec![], catalog)
    }

    /// Inserts a new MulteNode into the frontier if it is not dominated by any existing node.
    /// A node is dominated if there exists another node with a lower or equal cost.
    /// If the new node is inserted, any existing nodes that are dominated by it are removed.
    /// Returns true if the node was inserted, false otherwise.
    fn insert(&mut self, multe_node: MulteNode<QueryLang>) -> bool {
        // Check if the new node is dominated by any existing node
        for existing in &self.0 {
            if existing.cost <= multe_node.cost {
                return false; // New node is dominated, do not insert
            }
        }

        // Remove any existing nodes that are dominated by the new node
        self.0
            .retain(|existing| !(multe_node.cost <= existing.cost));

        // Insert the new node
        self.0.push(multe_node);
        true
    }

    /// Merges another FrontierAnalysis into this one.
    /// Returns true if the frontier was changed, false otherwise.
    fn merge(&mut self, other: Self) -> DidMerge {
        let mut changed = false;
        for multe_node in other.0 {
            if self.insert(multe_node) {
                changed = true;
            }
        }
        // Conservative, assume if self changed then both changed
        DidMerge(changed, changed)
    }
}

impl Analysis<QueryLang> for FrontierAnalysis {
    type Data = Self;

    fn make(egraph: &mut EGraph<QueryLang, Self>, enode: &QueryLang, id: Id) -> Self::Data {
        // When we add a new enode to the e-graph, we need to consider all possible combinations of all arguments
        // And create a new MulteNode for each combination, and insert it into the frontier
        // (which keeps only the non-dominated nodes).

        // Canonicalize children, since they may not be canonical when make is called.
        // Children are kept per argument position (not deduplicated), so an argument class that
        // appears in several positions can take a different cost from its frontier in each position.
        let children: Vec<Id> = enode.children().iter().map(|&c| egraph.find(c)).collect();

        // 1. Collect the frontier of costs for each argument position
        let arg_costs: Vec<Vec<CostProperties>> = children
            .iter()
            .map(|&c| egraph[c].data.0.iter().map(|m| m.cost.clone()).collect())
            .collect();

        // 2. Build every combination of argument costs (cartesian product).
        // A leaf has no arguments, so it gets exactly one (empty) combination.
        let mut combos: Vec<Vec<CostProperties>> = vec![vec![]];
        for costs in &arg_costs {
            combos = combos
                .iter()
                .flat_map(|prefix| {
                    costs.iter().map(move |c| {
                        let mut next = prefix.clone();
                        next.push(c.clone());
                        next
                    })
                })
                .collect();
        }

        // 3. Compute the cost of this enode for each combination and insert it into the frontier
        let mut frontier = FrontierAnalysis::new(egraph.analysis.1.clone());
        for combo in combos {
            let cost = frontier.1.op_cost(enode, &combo);
            let args: Vec<(Id, CostProperties)> = children.iter().copied().zip(combo).collect();
            frontier.insert(MulteNode {
                id,
                node: enode.clone(),
                args,
                cost,
            });
        }
        frontier
    }

    fn merge(&mut self, to: &mut Self::Data, from: Self::Data) -> DidMerge {
        // Merge the data from `from` into `to`.
        // Returns true if the frontier was changed, false otherwise.
        to.merge(from)
    }

    fn modify(egraph: &mut EGraph<QueryLang, Self>, id: Id) {
        // let data = egraph[id].data.clone();
        // egraph[id].data = data;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn c(materialized: bool, index: Option<&str>, cost: usize) -> CostProperties {
        CostProperties {
            tables: HashSet::new(),
            cols: HashSet::new(),
            rows: 0,
            index: index.map(String::from),
            materialized,
            cost,
        }
    }

    fn mn(cost: CostProperties) -> MulteNode<QueryLang> {
        MulteNode {
            id: Id::from(0usize),
            node: QueryLang::Name("n".into()),
            args: vec![],
            cost,
        }
    }

    fn frontier(costs: &[CostProperties]) -> FrontierAnalysis {
        let mut f = FrontierAnalysis::new(Catalog::new());
        for k in costs {
            f.insert(mn(k.clone()));
        }
        f
    }

    fn costs(f: &FrontierAnalysis) -> Vec<CostProperties> {
        f.0.iter().map(|m| m.cost.clone()).collect()
    }

    /// No two frontier entries are comparable, and every inserted cost is covered by some entry
    fn assert_valid_frontier(f: &FrontierAnalysis, inserted: &[CostProperties]) {
        for (i, a) in f.0.iter().enumerate() {
            for b in &f.0[i + 1..] {
                assert_eq!(
                    a.cost.partial_cmp(&b.cost),
                    None,
                    "comparable entries in frontier: {:?}",
                    costs(f)
                );
            }
        }
        for k in inserted {
            assert!(
                f.0.iter().any(|m| m.cost <= *k),
                "{k:?} not covered by frontier {:?}",
                costs(f)
            );
        }
    }

    #[test]
    fn insert_rejects_dominated_and_equal() {
        let mut f = frontier(&[c(true, None, 1)]);
        assert!(!f.insert(mn(c(false, None, 5))), "dominated");
        assert!(!f.insert(mn(c(true, None, 1))), "equal");
        assert_eq!(f.0.len(), 1);
    }

    #[test]
    fn insert_removes_dominated_entries() {
        let mut f = frontier(&[c(false, None, 5), c(false, Some("a"), 7), c(true, None, 9)]);
        assert_eq!(f.0.len(), 3);
        assert!(f.insert(mn(c(true, Some("a"), 1))));
        assert_eq!(costs(&f), vec![c(true, Some("a"), 1)]);
    }

    #[test]
    fn insert_keeps_incomparable_entries() {
        let f = frontier(&[
            c(true, None, 9),
            c(false, None, 1),
            c(false, Some("a"), 5),
            c(false, Some("b"), 5),
        ]);
        assert_eq!(f.0.len(), 4);
    }

    #[test]
    fn insert_maintains_frontier_invariant() {
        // Deterministic pseudo-random insertion sequences over a small cost domain
        let mut seed: u64 = 0x2545F4914F6CDD1D;
        let mut next = move |n: u64| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) % n
        };
        let indexes = [None, Some("a"), Some("b")];
        for _ in 0..200 {
            let mut f = FrontierAnalysis::new(Catalog::new());
            let mut inserted = vec![];
            for _ in 0..30 {
                let k = c(next(2) == 1, indexes[next(3) as usize], next(6) as usize);
                f.insert(mn(k.clone()));
                inserted.push(k);
                assert_valid_frontier(&f, &inserted);
            }
        }
    }

    #[test]
    fn merge_combines_frontiers() {
        let a = [c(true, None, 5), c(false, Some("a"), 5)];
        let b = [c(true, None, 3), c(false, Some("b"), 1)];
        let mut f = frontier(&a);
        let changed = f.merge(frontier(&b));
        assert!(changed.0);
        assert_valid_frontier(&f, &[a.clone(), b].concat());
        assert_eq!(f.0.len(), 3); // (true, None, 5) is dominated by (true, None, 3)

        // Merging in nothing new reports no change
        let mut g = frontier(&a);
        assert!(!g.merge(frontier(&[c(false, None, 9)])).0);
        assert_eq!(costs(&g), a.to_vec());
    }

    #[test]
    fn make_builds_frontier_per_argument_position() {
        let cat = Catalog::new().with_table("B".into(), 10);
        let mut eg: EGraph<QueryLang, FrontierAnalysis> = EGraph::new(FrontierAnalysis::new(cat));
        let b = eg.add(QueryLang::Name("B".into()));
        let x = eg.add(QueryLang::Name("x".into()));
        let cx = eg.add(QueryLang::Column([x]));
        assert_eq!(eg[b].data.0.len(), 1, "leaf gets exactly one entry");

        // A class holding two incomparable costs: Table(B) (materialized) and Index(B, x) (sorted)
        let tb = eg.add(QueryLang::Table([b]));
        let ib = eg.add(QueryLang::Index([b, x]));
        eg.union(tb, ib);
        eg.rebuild();
        let tb = eg.find(tb);
        assert_eq!(eg[tb].data.0.len(), 2);

        // The same class in both join positions: the best entry uses a different cost in each
        let j = eg.add(QueryLang::NestedLoopJoin([tb, tb, cx]));
        let f = &eg[j].data.0;
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].cost.index.as_deref(), Some("x"));
        assert_ne!(f[0].args[0].1, f[0].args[1].1);
        assert!(f[0].args.iter().map(|(i, _)| *i).eq([tb, tb, cx]));
    }
}
